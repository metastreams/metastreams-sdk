//! The `/v1/stream` socket: one [`StreamConnection`] carries many typed [`SubscriptionStream`]s.
//! <https://docs.metastreams.io/streams/overview> documents the protocol.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fmt;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll, ready};
use std::time::Duration;

use futures::{SinkExt, Stream, StreamExt};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{Instant, interval_at, sleep_until, timeout};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::{self, Message};
use tracing::{debug, warn};

use metastreams_types::candle::{CandleDenomination, CandleMetric};
use metastreams_types::error::ErrorDetail;
use metastreams_types::filter::CandleFilter;
use metastreams_types::stream::{
    ClientMessage, ServerMessage, SubscribeOptions, Subscription, SubscriptionId, Throttled,
};

use crate::constants::{
    CLOSE_GRACE, COMMAND_BUFFER, IDLE_TIMEOUT, PING_INTERVAL, SUBSCRIPTION_BUFFER,
};
use crate::error::{MetastreamsError, Result};

/// Why a subscription ends. Each one is transient except [`KeyRevoked`](Self::KeyRevoked) and
/// [`CreditsExhausted`](Self::CreditsExhausted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseReason {
    /// The server closes the socket with `4008` because the socket reads too slowly.
    SlowConsumer,
    /// This subscription's buffer is full, so the SDK closes this subscription only.
    Lagged,
    /// The server closes the socket with `1001` to drain for a restart.
    ServerRestarting,
    /// The server closes the socket with `1008` for a revoked key.
    KeyRevoked,
    /// The server closes the socket with `4402` because the organization's credits this billing
    /// period reach its plan's limit.
    CreditsExhausted,
    /// The server closes the socket with a code this build does not name.
    Other {
        /// The close code.
        code: u16,
        /// The close reason text.
        reason: String,
    },
    /// The socket ends without a close the server names.
    Lost(LostCause),
    /// This side closes the socket with [`StreamConnection::close`] or by dropping the connection.
    Client,
}

impl CloseReason {
    /// Whether a new socket can carry the subscription again.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        !matches!(self, Self::KeyRevoked | Self::CreditsExhausted)
    }

    /// The reason for a task that ends without publishing one, such as by a panic.
    fn task_ended() -> Self {
        Self::Lost(LostCause::TaskEnded)
    }

    fn from_frame(frame: Option<CloseFrame>) -> Self {
        let Some(frame) = frame else {
            return Self::Lost(LostCause::NoCloseCode);
        };
        match u16::from(frame.code) {
            1001 => Self::ServerRestarting,
            1008 => Self::KeyRevoked,
            4008 => Self::SlowConsumer,
            4402 => Self::CreditsExhausted,
            code => Self::Other { code, reason: frame.reason.as_str().to_owned() },
        }
    }
}

impl fmt::Display for CloseReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SlowConsumer => f.write_str("slow consumer (4008)"),
            Self::Lagged => f.write_str("the subscription's buffer filled"),
            Self::ServerRestarting => f.write_str("server restarting (1001)"),
            Self::KeyRevoked => f.write_str("key revoked (1008)"),
            Self::CreditsExhausted => f.write_str("credits exhausted (4402)"),
            Self::Other { code, reason } => write!(f, "{reason} ({code})"),
            Self::Lost(cause) => write!(f, "connection lost: {cause}"),
            Self::Client => f.write_str("closed by the client"),
        }
    }
}

/// Why a socket ends without a close the server names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LostCause {
    /// The server sends a close frame without a code.
    NoCloseCode,
    /// The connection ends without a close frame.
    Ended,
    /// The server sends no frame within the idle timeout, not even a `pong`.
    IdleTimeout,
    /// A read or write on the socket fails.
    Transport(String),
    /// The SDK's socket task ends without a reason, such as by a panic.
    TaskEnded,
}

impl fmt::Display for LostCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoCloseCode => f.write_str("the server closed without a code"),
            Self::Ended => f.write_str("the socket ended"),
            Self::IdleTimeout => f.write_str("no frame within the idle timeout"),
            Self::Transport(error) => f.write_str(error),
            Self::TaskEnded => f.write_str("the stream task ended"),
        }
    }
}

/// One open `/v1/stream` socket.
///
/// The socket lives as long as this handle; share it behind an `Arc`. It never reconnects.
#[derive(Debug)]
pub struct StreamConnection {
    commands: mpsc::Sender<Command>,
    closed: watch::Receiver<Option<CloseReason>>,
}

impl StreamConnection {
    /// Hands `socket` to a task that routes its frames until it closes.
    pub(crate) fn spawn<T>(socket: WebSocketStream<T>) -> Self
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let (commands, inbox) = mpsc::channel(COMMAND_BUFFER);
        let (closing, closed) = watch::channel(None);
        let driver = Driver {
            socket,
            inbox,
            commands: commands.downgrade(),
            pending: HashMap::new(),
            routes: HashMap::new(),
            last_request: 0,
            closing,
        };
        tokio::spawn(driver.run());
        Self { commands, closed }
    }

    /// Opens one subscription and waits for its ack. Its first update is the next live change.
    /// The stream sends no snapshot, so read the REST view after the ack.
    ///
    /// # Errors
    /// `Rejected` when the server refuses the filter or the key holds its most
    /// subscriptions; `Closed` when the socket is gone.
    pub async fn subscribe<F: Subscription>(&self, filter: &F) -> Result<SubscriptionStream<F>> {
        self.open(filter, SubscribeOptions::default()).await
    }

    /// Opens one subscription that sends each token at most once per `throttle`.
    /// An exit skips the throttle.
    ///
    /// # Errors
    /// `Rejected` with `VALIDATION_ERROR` when `throttle` is above the server's limit;
    /// otherwise as [`subscribe`](Self::subscribe).
    pub async fn subscribe_throttled<F: Throttled>(
        &self,
        filter: &F,
        throttle: Duration,
    ) -> Result<SubscriptionStream<F>> {
        let options =
            SubscribeOptions { throttle_ms: Some(throttle_ms(throttle)), ..Default::default() };
        self.open(filter, options).await
    }

    /// Opens one `spot.candles` subscription, shaped by `options`.
    ///
    /// # Errors
    /// As [`subscribe_throttled`](Self::subscribe_throttled).
    pub async fn subscribe_candles(
        &self,
        filter: &CandleFilter,
        options: CandleOptions,
    ) -> Result<SubscriptionStream<CandleFilter>> {
        let options = SubscribeOptions {
            throttle_ms: options.throttle.map(throttle_ms),
            closed_only: options.closed_only.then_some(true),
            metric: Some(options.metric),
            denomination: Some(options.denomination),
        };
        self.open(filter, options).await
    }

    /// Sends the subscribe and waits for its ack or its refusal.
    async fn open<F: Subscription>(
        &self,
        filter: &F,
        options: SubscribeOptions,
    ) -> Result<SubscriptionStream<F>> {
        let filters = serde_json::to_value(filter).expect("a filter serializes to JSON");
        let (reply, answer) = oneshot::channel();
        let command = Command::Subscribe { channel: F::CHANNEL, filters, options, reply };
        self.commands.send(command).await.map_err(|_| self.closed_error())?;
        let opened = answer.await.map_err(|_| self.closed_error())??;
        Ok(SubscriptionStream { opened, finished: false, filter: PhantomData })
    }

    /// Waits until the socket closes, and says why. Every subscription on it ends then too.
    pub async fn closed(&self) -> CloseReason {
        wait_closed(self.closed.clone()).await
    }

    /// Sends the close frame; every subscription on the socket ends with `Closed(Client)`.
    pub async fn close(self) {
        drop(self.commands);
        wait_closed(self.closed).await;
    }

    fn closed_error(&self) -> MetastreamsError {
        let reason = self.closed.borrow().clone();
        MetastreamsError::Closed(reason.unwrap_or_else(CloseReason::task_ended))
    }
}

/// One frame of a subscription, in the order the server sends it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event<U, E> {
    /// A payload that matches the filter.
    Update(U),
    /// Something the subscription matched leaves it: it stops matching, or the server's tracking
    /// cap evicts it. The subscription stays open, and a later match arrives as a full update.
    Exit(E),
}

/// One subscription's frames, each decoded as an [`Event`] of its channel's payloads.
///
/// It ends with one `Err(Closed)` that says why. Dropping it unsubscribes. A frame
/// that does not decode yields `Err(FrameDecode)`, and the stream goes on.
pub struct SubscriptionStream<F: Subscription> {
    opened: Opened,
    finished: bool,
    filter: PhantomData<fn() -> F>,
}

impl<F: Subscription> SubscriptionStream<F> {
    /// The id the server assigns; every update and exit for this subscription carries it.
    #[must_use]
    pub fn id(&self) -> SubscriptionId {
        self.opened.id
    }
}

impl<F: Subscription> Stream for SubscriptionStream<F> {
    type Item = Result<Event<F::Update, F::Exit>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.finished {
            return Poll::Ready(None);
        }
        let Some(frame) = ready!(self.opened.frames.poll_recv(cx)) else {
            self.finished = true;
            let reason = self.opened.ended.get().cloned().unwrap_or_else(CloseReason::task_ended);
            return Poll::Ready(Some(Err(MetastreamsError::Closed(reason))));
        };
        Poll::Ready(Some(frame.decode::<F>()))
    }
}

impl<F: Subscription> fmt::Debug for SubscriptionStream<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SubscriptionStream")
            .field("channel", &F::CHANNEL)
            .field("id", &self.opened.id)
            .finish_non_exhaustive()
    }
}

/// What a `spot.candles` subscription asks beside its filter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CandleOptions {
    /// What the values measure.
    pub metric: CandleMetric,
    /// What the values are denominated in.
    pub denomination: CandleDenomination,
    /// Whether to receive only finished buckets.
    pub closed_only: bool,
    /// The shortest gap between two updates of one series; `None` sends every update.
    pub throttle: Option<Duration>,
}

/// A request from a handle to the task that owns the socket.
enum Command {
    Subscribe {
        channel: &'static str,
        filters: Value,
        options: SubscribeOptions,
        reply: oneshot::Sender<Result<Opened>>,
    },
    Unsubscribe {
        id: SubscriptionId,
    },
}

/// One subscription's frame, routed and not yet decoded.
enum Frame {
    Update(Value),
    Exit(Value),
}

impl Frame {
    /// Reads the frame as `F`'s update or exit payload.
    fn decode<F: Subscription>(self) -> Result<Event<F::Update, F::Exit>> {
        let event = match self {
            Self::Update(data) => serde_json::from_value(data).map(Event::Update),
            Self::Exit(data) => serde_json::from_value(data).map(Event::Exit),
        };
        event.map_err(|source| MetastreamsError::FrameDecode { channel: F::CHANNEL, source })
    }
}

/// What an acked subscribe hands back to its caller. Dropping it unsubscribes, even
/// unread in a reply whose caller left. An unsubscribe that a full inbox refuses
/// goes out on the next frame instead.
struct Opened {
    id: SubscriptionId,
    frames: mpsc::Receiver<Frame>,
    ended: Arc<OnceLock<CloseReason>>,
    /// Weak, so that a live subscription does not hold the socket open.
    commands: mpsc::WeakSender<Command>,
}

impl Drop for Opened {
    fn drop(&mut self) {
        if self.ended.get().is_none()
            && let Some(commands) = self.commands.upgrade()
        {
            let _ = commands.try_send(Command::Unsubscribe { id: self.id });
        }
    }
}

/// A subscribe sent and not yet answered.
struct Pending {
    channel: &'static str,
    reply: oneshot::Sender<Result<Opened>>,
}

/// Where one subscription's frames go.
struct Route {
    channel: &'static str,
    frames: mpsc::Sender<Frame>,
    ended: Arc<OnceLock<CloseReason>>,
}

/// The task that owns the socket: it routes each update and exit to its subscription.
struct Driver<T> {
    socket: WebSocketStream<T>,
    inbox: mpsc::Receiver<Command>,
    /// Weak, so that the task does not hold its own inbox open.
    commands: mpsc::WeakSender<Command>,
    /// Unanswered subscribes, keyed by their request id.
    pending: HashMap<String, Pending>,
    routes: HashMap<SubscriptionId, Route>,
    last_request: u64,
    closing: watch::Sender<Option<CloseReason>>,
}

impl<T: AsyncRead + AsyncWrite + Unpin + Send + 'static> Driver<T> {
    async fn run(mut self) {
        let reason = self.serve().await;
        debug!(%reason, "metastreams stream closed");
        self.closing.send_replace(Some(reason.clone()));
        for (_, pending) in self.pending.drain() {
            let _ = pending.reply.send(Err(MetastreamsError::Closed(reason.clone())));
        }
        for (_, route) in self.routes.drain() {
            let _ = route.ended.set(reason.clone());
        }
    }

    async fn serve(&mut self) -> CloseReason {
        let mut ping = interval_at(Instant::now() + PING_INTERVAL, PING_INTERVAL);
        let mut last_heard = Instant::now();
        // A frame moves only `last_heard`. The timer moves to the new deadline when it fires.
        let idle = sleep_until(last_heard + IDLE_TIMEOUT);
        tokio::pin!(idle);
        loop {
            let outcome = tokio::select! {
                frame = self.socket.next() => {
                    last_heard = Instant::now();
                    match frame {
                        Some(Ok(Message::Text(text))) => self.on_text(text.as_str()).await,
                        Some(Ok(Message::Close(frame))) => {
                            // tungstenite queues the reply to a close, and only a flush sends it.
                            let _ = timeout(CLOSE_GRACE, self.socket.flush()).await;
                            return CloseReason::from_frame(frame);
                        }
                        Some(Ok(_)) => Ok(()),
                        Some(Err(error)) => Err(error),
                        None => return CloseReason::Lost(LostCause::Ended),
                    }
                }
                command = self.inbox.recv() => match command {
                    Some(command) => self.on_command(command).await,
                    None => {
                        self.close().await;
                        return CloseReason::Client;
                    }
                },
                () = &mut idle => {
                    let deadline = last_heard + IDLE_TIMEOUT;
                    if deadline <= Instant::now() {
                        return CloseReason::Lost(LostCause::IdleTimeout);
                    }
                    idle.as_mut().reset(deadline);
                    Ok(())
                }
                _ = ping.tick() => self.send(&ClientMessage::Ping).await,
            };
            if let Err(error) = outcome {
                return CloseReason::Lost(LostCause::Transport(error.to_string()));
            }
        }
    }

    async fn on_command(&mut self, command: Command) -> tungstenite::Result<()> {
        match command {
            Command::Subscribe { channel, filters, options, reply } => {
                self.last_request += 1;
                let id = self.last_request.to_string();
                self.pending.insert(id.clone(), Pending { channel, reply });
                self.send(&ClientMessage::Subscribe {
                    channel: channel.to_owned(),
                    filters,
                    id: Some(id),
                    options,
                })
                .await
            }
            Command::Unsubscribe { id } => {
                // A lagged or already-unsubscribed subscription has no route: its unsubscribe went out.
                let Some(route) = self.routes.remove(&id) else {
                    return Ok(());
                };
                self.send(&unsubscribe(route.channel, id)).await
            }
        }
    }

    async fn on_text(&mut self, text: &str) -> tungstenite::Result<()> {
        let message = match serde_json::from_str::<ServerMessage>(text) {
            Ok(message) => message,
            Err(error) => {
                warn!(%error, "metastreams sent an unreadable frame");
                return Ok(());
            }
        };
        match message {
            ServerMessage::Update { subscription_id, data, .. } => {
                self.deliver(subscription_id, Frame::Update(data)).await
            }
            ServerMessage::Exit { subscription_id, data, .. } => {
                self.deliver(subscription_id, Frame::Exit(data)).await
            }
            ServerMessage::Ack { id: Some(request), subscription_id: Some(id), .. } => {
                self.open(&request, id);
                Ok(())
            }
            ServerMessage::Error { id: Some(request), error } => {
                self.reject(&request, error);
                Ok(())
            }
            ServerMessage::Error { id: None, error } => {
                warn!(code = ?error.code, message = %error.message, "metastreams refused a frame");
                Ok(())
            }
            ServerMessage::Ack { .. } | ServerMessage::Pong | ServerMessage::Unknown => Ok(()),
        }
    }

    /// Routes the ack of subscribe `request` to its caller. A caller that left drops the
    /// `Opened`, and that drop unsubscribes it.
    fn open(&mut self, request: &str, id: SubscriptionId) {
        let Some(Pending { channel, reply }) = self.pending.remove(request) else {
            return;
        };
        let (frames, receiver) = mpsc::channel(SUBSCRIPTION_BUFFER);
        let ended = Arc::new(OnceLock::new());
        self.routes.insert(id, Route { channel, frames, ended: Arc::clone(&ended) });
        let commands = self.commands.clone();
        let _ = reply.send(Ok(Opened { id, frames: receiver, ended, commands }));
    }

    /// Hands the refusal of subscribe `request` to its caller.
    fn reject(&mut self, request: &str, error: ErrorDetail) {
        let Some(Pending { channel, reply }) = self.pending.remove(request) else {
            return;
        };
        let _ = reply.send(Err(MetastreamsError::Rejected { channel, error: Box::new(error) }));
    }

    /// Hands `frame` to subscription `id` without waiting, so one unread subscription
    /// cannot stall the others.
    async fn deliver(&mut self, id: SubscriptionId, frame: Frame) -> tungstenite::Result<()> {
        // A frame can race its own unsubscribe.
        let Entry::Occupied(route) = self.routes.entry(id) else {
            return Ok(());
        };
        match route.get().frames.try_send(frame) {
            Ok(()) => Ok(()),
            Err(TrySendError::Closed(_)) => {
                let route = route.remove();
                self.send(&unsubscribe(route.channel, id)).await
            }
            Err(TrySendError::Full(_)) => {
                let route = route.remove();
                let _ = route.ended.set(CloseReason::Lagged);
                warn!(channel = route.channel, ?id, "metastreams subscription lagged; closing it");
                self.send(&unsubscribe(route.channel, id)).await
            }
        }
    }

    async fn send(&mut self, message: &ClientMessage) -> tungstenite::Result<()> {
        let text = serde_json::to_string(message).expect("a client frame serializes to JSON");
        self.socket.send(Message::text(text)).await
    }

    /// Sends a normal close, then reads until the server's echo.
    async fn close(&mut self) {
        let normal = CloseFrame { code: CloseCode::Normal, reason: "".into() };
        let _ = timeout(CLOSE_GRACE, async {
            // A socket dropped before the echo can reset the server's read of the close.
            if self.socket.close(Some(normal)).await.is_ok() {
                while let Some(Ok(_)) = self.socket.next().await {}
            }
        })
        .await;
    }
}

/// `throttle` in whole milliseconds, rounded up so the server's gap is never shorter. Rounded
/// down, a gap below 1 ms is 0, and 0 sends every update.
fn throttle_ms(throttle: Duration) -> u64 {
    u64::try_from(throttle.as_nanos().div_ceil(1_000_000)).unwrap_or(u64::MAX)
}

fn unsubscribe(channel: &'static str, id: SubscriptionId) -> ClientMessage {
    ClientMessage::Unsubscribe { channel: channel.to_owned(), subscription_id: Some(id) }
}

/// Waits for the reason the socket's task publishes as it ends.
async fn wait_closed(mut closed: watch::Receiver<Option<CloseReason>>) -> CloseReason {
    match closed.wait_for(Option::is_some).await {
        Ok(reason) => reason.clone().unwrap_or_else(CloseReason::task_ended),
        Err(_) => CloseReason::task_ended(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::future::Future;

    use futures::poll;
    use serde_json::json;
    use tokio::io::AsyncReadExt;
    use tokio::net::{TcpListener, TcpStream};
    use tokio::task::JoinHandle;
    use tokio_tungstenite::accept_hdr_async;
    use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
    use tokio_tungstenite::tungstenite::protocol::Role;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use metastreams_types::address::MultiChainAddress;
    use metastreams_types::candle::Timeframe;
    use metastreams_types::chain::SpotChain;
    use metastreams_types::error::ErrorCode;
    use metastreams_types::filter::{Range, TokenFilter, TradeFilter};
    use metastreams_types::stream::{AckStatus, UiTokenExit};
    use metastreams_types::trade::{TradeDirection, TradeFlag};

    use crate::MetastreamsProvider;

    const KEY: &str = "meta_test";
    const MINT: &str = "So11111111111111111111111111111111111111112";
    const WALLET: &str = "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin";
    const AT: i64 = 1_700_000_000_000;

    type ServerSocket = WebSocketStream<TcpStream>;

    /// A one-socket server: it checks the upgrade, then runs `script` on the socket.
    async fn serve<S, Fut>(script: S) -> (MetastreamsProvider, JoinHandle<()>)
    where
        S: FnOnce(ServerSocket) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
        let address = listener.local_addr().expect("an address");
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.expect("a client");
            let check = |request: &Request, response: Response| {
                assert_eq!(request.uri().path(), "/v1/stream");
                let bearer = request.headers()["authorization"].to_str().expect("an ASCII header");
                assert_eq!(bearer, format!("Bearer {KEY}"));
                Ok(response)
            };
            script(accept_hdr_async(tcp, check).await.expect("an upgrade")).await;
        });
        let provider = MetastreamsProvider::new_with_base_url(KEY, &format!("http://{address}"))
            .expect("provider");
        (provider, server)
    }

    /// The next text frame, as the client sends it.
    async fn read_json(socket: &mut ServerSocket) -> Value {
        loop {
            if let Message::Text(text) = socket.next().await.expect("a frame").expect("a read") {
                return serde_json::from_str(text.as_str()).expect("a JSON frame");
            }
        }
    }

    async fn read(socket: &mut ServerSocket) -> ClientMessage {
        serde_json::from_value(read_json(socket).await).expect("a client frame")
    }

    async fn write(socket: &mut ServerSocket, message: &ServerMessage) {
        let text = serde_json::to_string(message).expect("a server frame");
        socket.send(Message::text(text)).await.expect("a write");
    }

    /// Acks the next subscribe. Returns the id it assigns and the subscribe as sent.
    async fn ack_subscribe(socket: &mut ServerSocket) -> (SubscriptionId, Value) {
        let frame = read_json(socket).await;
        let subscribe: ClientMessage =
            serde_json::from_value(frame.clone()).expect("a client frame");
        let ClientMessage::Subscribe { channel, id, .. } = subscribe else {
            panic!("expected a subscribe, got {frame}");
        };
        let subscription_id = SubscriptionId::new();
        let ack = ServerMessage::Ack {
            channel,
            subscription_id: Some(subscription_id),
            id,
            status: AckStatus::Success,
            message: "subscribe successful".to_owned(),
        };
        write(socket, &ack).await;
        (subscription_id, frame)
    }

    /// Acks the next subscribe and returns the id it assigns.
    async fn ack(socket: &mut ServerSocket) -> SubscriptionId {
        ack_subscribe(socket).await.0
    }

    async fn update<F: Subscription>(socket: &mut ServerSocket, id: SubscriptionId, data: Value) {
        let update =
            ServerMessage::Update { channel: F::CHANNEL.into(), subscription_id: id, data };
        write(socket, &update).await;
    }

    async fn exit<F: Subscription>(socket: &mut ServerSocket, id: SubscriptionId, data: Value) {
        let exit = ServerMessage::Exit { channel: F::CHANNEL.into(), subscription_id: id, data };
        write(socket, &exit).await;
    }

    /// Holds the socket open until the client closes it.
    async fn drain(mut socket: ServerSocket) {
        while let Some(Ok(_)) = socket.next().await {}
    }

    /// One mint's one-minute candles.
    fn candles() -> CandleFilter {
        CandleFilter {
            tokens: Some(vec![MINT.parse::<MultiChainAddress>().expect("a valid mint")]),
            timeframes: vec![Timeframe::OneMin],
            ..Default::default()
        }
    }

    fn candle(open: f64) -> Value {
        json!({
            "chain": "solana", "token": MINT, "pool": null, "timeframe": "1m",
            "time": AT, "open": open, "high": 2.0, "low": 0.5, "close": 0.7, "volume": 10.0
        })
    }

    /// Solana tokens worth at least $1M.
    fn large_caps() -> TokenFilter {
        let floor = "1000000".parse().expect("a decimal");
        TokenFilter {
            chains: vec![SpotChain::Solana],
            market_cap_usd: Some(Range { min: Some(floor), max: None }),
            ..Default::default()
        }
    }

    /// KOL trades of the tokens [`large_caps`] keeps.
    fn kol_trades() -> TradeFilter {
        TradeFilter { flags: vec![TradeFlag::Kol], token: Some(large_caps()), ..Default::default() }
    }

    /// A `spot.tokens` payload for `MINT` with every required field.
    fn token_payload() -> Value {
        let money = json!({ "native": "1", "usd": "150" });
        let window = json!({
            "priceChangePct": "0",
            "volume": money,
            "buyCount": 0,
            "sellCount": 0,
            "buyVolume": money,
            "sellVolume": money,
        });
        json!({
            "chain": "solana",
            "address": MINT,
            "name": "Wrapped SOL",
            "symbol": "SOL",
            "decimals": 9,
            "supply": "1000000000",
            "creator": WALLET,
            "devFunding": { "status": "unresolved" },
            "createdAt": AT,
            "dexes": ["pumpswap"],
            "price": money,
            "marketCap": money,
            "liquidity": money,
            "ath": { "price": money, "priceAt": AT, "marketCap": money, "marketCapAt": AT },
            "stats": { "5m": window, "30m": window, "1h": window, "6h": window, "24h": window },
            "totals": {
                "volume": money,
                "fees": { "transaction": money, "validator": money, "platform": money },
            },
            "holders": {
                "count": 0,
                "top10Pct": "0",
                "devPct": "0",
                "sniperPct": "0",
                "sniperCount": 0,
                "bundlerPct": "0",
                "bundlerCount": 0,
                "insiderPct": "0",
                "insiderCount": 0,
                "phishingPct": "0",
                "phishingCount": 0,
                "freshWalletPct": "0",
                "freshWalletCount": 0,
                "fomoPct": "0",
                "fomoCount": 0,
                "proTraderPct": "0",
                "proTraderCount": 0,
                "fundedByCexPct": "0",
            },
            "security": { "mintDisabled": true, "transferBlockable": false, "rugPct": "0" },
            "metadata": { "twitterReuseCount": 0 },
            "updatedAt": AT,
        })
    }

    /// A `spot.trades` payload: a buy of `MINT`.
    fn trade_payload() -> Value {
        let money = json!({ "native": "1", "usd": "150" });
        json!({
            "id": "trade-1",
            "chain": "solana",
            "token": MINT,
            "name": "Wrapped SOL",
            "symbol": "SOL",
            "trader": WALLET,
            "transactionHash": "signature-1",
            "side": "buy",
            "amount": "1250.5",
            "price": money,
            "volume": money,
            "marketCap": money,
            "timestamp": AT,
        })
    }

    fn mint_exit() -> Value {
        json!({ "chain": "solana", "address": MINT })
    }

    #[tokio::test]
    async fn a_subscription_reads_its_updates_and_skips_an_unreadable_one() -> Result<()> {
        let (provider, _server) = serve(|mut socket| async move {
            let id = ack(&mut socket).await;
            update::<CandleFilter>(&mut socket, id, candle(1.5)).await;
            update::<CandleFilter>(&mut socket, id, json!({ "open": "not a candle" })).await;
            update::<CandleFilter>(&mut socket, id, candle(2.5)).await;
            drain(socket).await;
        })
        .await;

        let connection = provider.connect().await?;
        let mut candles = connection.subscribe(&candles()).await?;

        // `spot.candles` sends no exit, so the pattern needs no `Exit` arm.
        let Event::Update(first) = candles.next().await.expect("an item")?;
        assert!((first.candle.open - 1.5).abs() < f64::EPSILON);
        let unreadable = candles.next().await.expect("an item").expect_err("a bad payload");
        assert!(matches!(
            unreadable,
            MetastreamsError::FrameDecode { channel: "spot.candles", .. }
        ));
        let Event::Update(third) = candles.next().await.expect("an item")?;
        assert!((third.candle.open - 2.5).abs() < f64::EPSILON);
        Ok(())
    }

    #[tokio::test]
    async fn an_exit_on_a_channel_without_exits_is_unreadable() {
        let (provider, _server) = serve(|mut socket| async move {
            let id = ack(&mut socket).await;
            exit::<CandleFilter>(&mut socket, id, mint_exit()).await;
            update::<CandleFilter>(&mut socket, id, candle(1.5)).await;
            drain(socket).await;
        })
        .await;

        let connection = provider.connect().await.expect("a socket");
        let mut candles = connection.subscribe(&candles()).await.expect("an ack");

        let unreadable = candles.next().await.expect("an item").expect_err("no exit reads");
        assert!(matches!(
            unreadable,
            MetastreamsError::FrameDecode { channel: "spot.candles", .. }
        ));
        let Event::Update(next) = candles.next().await.expect("an item").expect("a candle");
        assert!((next.candle.open - 1.5).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn a_token_that_stops_matching_exits_and_can_match_again() {
        let (subscribed, on_subscribe) = oneshot::channel();
        let (provider, _server) = serve(|mut socket| async move {
            let (id, subscribe) = ack_subscribe(&mut socket).await;
            subscribed.send(subscribe).expect("the test waits");
            update::<TokenFilter>(&mut socket, id, token_payload()).await;
            exit::<TokenFilter>(&mut socket, id, mint_exit()).await;
            update::<TokenFilter>(&mut socket, id, token_payload()).await;
            drain(socket).await;
        })
        .await;

        let connection = provider.connect().await.expect("a socket");
        let mut tokens = connection.subscribe(&large_caps()).await.expect("an ack");

        let subscribe = on_subscribe.await.expect("a subscribe");
        assert_eq!(subscribe["channel"], "spot.tokens");
        assert_eq!(
            subscribe["filters"],
            json!({ "chains": ["solana"], "marketCapUsd": { "min": "1000000" } })
        );
        let first = tokens.next().await.expect("an item").expect("a token");
        assert!(matches!(&first, Event::Update(token) if token.address == MINT), "{first:?}");
        let gone = tokens.next().await.expect("an item").expect("an exit");
        let exit = UiTokenExit { chain: SpotChain::Solana, address: MINT.to_owned() };
        assert_eq!(gone, Event::Exit(exit));
        let again = tokens.next().await.expect("an item").expect("a token");
        assert!(matches!(&again, Event::Update(token) if token.address == MINT), "{again:?}");
    }

    #[tokio::test]
    async fn a_throttled_subscribe_sends_its_gap_and_a_plain_one_omits_it() {
        let (subscribed, on_subscribe) = oneshot::channel();
        let (provider, _server) = serve(|mut socket| async move {
            let (_, throttled) = ack_subscribe(&mut socket).await;
            let (_, plain) = ack_subscribe(&mut socket).await;
            subscribed.send((throttled, plain)).expect("the test waits");
            drain(socket).await;
        })
        .await;

        let connection = provider.connect().await.expect("a socket");
        let filter = large_caps();
        let throttle = Duration::from_millis(250);
        let _throttled = connection.subscribe_throttled(&filter, throttle).await.expect("an ack");
        let _plain = connection.subscribe(&filter).await.expect("an ack");

        let (throttled, plain) = on_subscribe.await.expect("both subscribes");
        assert_eq!(throttled["throttleMs"], 250);
        assert!(plain.get("throttleMs").is_none(), "{plain}");
    }

    #[tokio::test]
    async fn a_candle_subscribe_sends_its_options_beside_the_filter() {
        let (subscribed, on_subscribe) = oneshot::channel();
        let (provider, _server) = serve(|mut socket| async move {
            let (_, frame) = ack_subscribe(&mut socket).await;
            subscribed.send(frame).expect("the test waits");
            drain(socket).await;
        })
        .await;

        let connection = provider.connect().await.expect("a socket");
        let options = CandleOptions {
            metric: CandleMetric::Price,
            denomination: CandleDenomination::Native,
            closed_only: true,
            throttle: Some(Duration::from_millis(500)),
        };
        let _candles = connection.subscribe_candles(&candles(), options).await.expect("an ack");

        let frame = on_subscribe.await.expect("a subscribe");
        assert_eq!(frame["channel"], "spot.candles");
        assert_eq!(frame["filters"], json!({ "tokens": [MINT], "timeframes": ["1m"] }));
        assert_eq!(
            (&frame["metric"], &frame["denomination"], &frame["closedOnly"], &frame["throttleMs"]),
            (&json!("price"), &json!("native"), &json!(true), &json!(500))
        );
    }

    #[tokio::test]
    async fn a_throttle_between_whole_milliseconds_rounds_up() {
        let (subscribed, on_subscribe) = oneshot::channel();
        let (provider, _server) = serve(|mut socket| async move {
            let (_, fractional) = ack_subscribe(&mut socket).await;
            let (_, tiny) = ack_subscribe(&mut socket).await;
            subscribed.send((fractional, tiny)).expect("the test waits");
            drain(socket).await;
        })
        .await;

        let connection = provider.connect().await.expect("a socket");
        let filter = large_caps();
        let fractional = Duration::from_micros(250_500);
        let _fractional =
            connection.subscribe_throttled(&filter, fractional).await.expect("an ack");
        let tiny = Duration::from_micros(1);
        let _tiny = connection.subscribe_throttled(&filter, tiny).await.expect("an ack");

        let (fractional, tiny) = on_subscribe.await.expect("both subscribes");
        assert_eq!(fractional["throttleMs"], 251);
        assert_eq!(tiny["throttleMs"], 1, "0 would send every update");
    }

    #[tokio::test]
    async fn a_full_buffer_ends_only_its_subscription_after_the_frames_it_holds() {
        let (provider, _server) = serve(|mut socket| async move {
            let slow = ack(&mut socket).await;
            let fast = ack(&mut socket).await;
            // One update and its exits fill the buffer; one exit more overflows it.
            update::<TokenFilter>(&mut socket, slow, token_payload()).await;
            for _ in 0..SUBSCRIPTION_BUFFER {
                exit::<TokenFilter>(&mut socket, slow, mint_exit()).await;
            }
            update::<CandleFilter>(&mut socket, fast, candle(1.5)).await;
            let unsubscribe = read(&mut socket).await;
            let expected = ClientMessage::Unsubscribe {
                channel: TokenFilter::CHANNEL.to_owned(),
                subscription_id: Some(slow),
            };
            assert_eq!(unsubscribe, expected);
            update::<CandleFilter>(&mut socket, fast, candle(2.5)).await;
            drain(socket).await;
        })
        .await;

        let connection = provider.connect().await.expect("a socket");
        let mut slow = connection.subscribe(&large_caps()).await.expect("an ack");
        let mut fast = connection.subscribe(&candles()).await.expect("an ack");

        // The socket is in order, so the driver has routed every slow frame by now.
        let Event::Update(first) = fast.next().await.expect("an item").expect("a candle");
        assert!((first.candle.open - 1.5).abs() < f64::EPSILON);
        let held = slow.next().await.expect("an item").expect("a token");
        assert!(matches!(&held, Event::Update(token) if token.address == MINT), "{held:?}");
        let exit = Event::Exit(UiTokenExit { chain: SpotChain::Solana, address: MINT.to_owned() });
        for _ in 1..SUBSCRIPTION_BUFFER {
            assert_eq!(slow.next().await.expect("an item").expect("an exit"), exit);
        }
        let end = slow.next().await.expect("an item").expect_err("a lag");
        assert!(matches!(&end, MetastreamsError::Closed(CloseReason::Lagged)), "{end:?}");
        assert!(end.is_transient());
        assert!(slow.next().await.is_none());
        // The server sends this only after it reads the slow subscription's unsubscribe.
        let Event::Update(second) = fast.next().await.expect("an item").expect("a candle");
        assert!((second.candle.open - 2.5).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn a_trades_subscription_reads_a_typed_trade() -> Result<()> {
        let (subscribed, on_subscribe) = oneshot::channel();
        let (provider, _server) = serve(|mut socket| async move {
            let (id, subscribe) = ack_subscribe(&mut socket).await;
            subscribed.send(subscribe).expect("the test waits");
            update::<TradeFilter>(&mut socket, id, trade_payload()).await;
            drain(socket).await;
        })
        .await;

        let connection = provider.connect().await?;
        let mut trades = connection.subscribe(&kol_trades()).await?;

        let subscribe = on_subscribe.await.expect("a subscribe");
        assert_eq!(subscribe["channel"], "spot.trades");
        assert_eq!(
            subscribe["filters"],
            json!({
                "flags": ["kol"],
                "token": { "chains": ["solana"], "marketCapUsd": { "min": "1000000" } },
            })
        );
        let Event::Update(trade) = trades.next().await.expect("an item")?;
        assert_eq!(trade.token, MINT);
        assert_eq!(trade.side, TradeDirection::Buy);
        Ok(())
    }

    #[tokio::test]
    async fn a_refused_subscribe_is_rejected_with_its_code() {
        let (provider, _server) = serve(|mut socket| async move {
            let ClientMessage::Subscribe { id, .. } = read(&mut socket).await else {
                panic!("expected a subscribe");
            };
            let error = ServerMessage::Error {
                id,
                error: ErrorDetail {
                    code: ErrorCode::ValidationError,
                    message: "This key holds the most subscriptions it may.".to_owned(),
                    details: None,
                },
            };
            write(&mut socket, &error).await;
            drain(socket).await;
        })
        .await;

        let connection = provider.connect().await.expect("a socket");
        let error = connection.subscribe(&candles()).await.expect_err("a refusal");

        let MetastreamsError::Rejected { channel, error: detail } = &error else {
            panic!("expected a rejection, got {error:?}");
        };
        assert_eq!(*channel, "spot.candles");
        assert_eq!(detail.code, ErrorCode::ValidationError);
        assert!(!error.is_transient());
    }

    #[tokio::test]
    async fn a_server_close_ends_every_subscription_with_its_reason() {
        let (provider, _server) = serve(|mut socket| async move {
            ack(&mut socket).await;
            ack(&mut socket).await;
            let close = CloseFrame { code: CloseCode::from(4008), reason: "slow consumer".into() };
            socket.close(Some(close)).await.expect("a close");
            drain(socket).await;
        })
        .await;

        let connection = provider.connect().await.expect("a socket");
        let mut first = connection.subscribe(&candles()).await.expect("an ack");
        let mut second = connection.subscribe(&candles()).await.expect("an ack");

        for stream in [&mut first, &mut second] {
            let end = stream.next().await.expect("an item").expect_err("a close");
            assert!(matches!(&end, MetastreamsError::Closed(CloseReason::SlowConsumer)));
            assert!(end.is_transient());
            assert!(stream.next().await.is_none());
        }
        let late = connection.subscribe(&candles()).await.expect_err("a closed socket");
        assert!(matches!(late, MetastreamsError::Closed(CloseReason::SlowConsumer)));
        assert_eq!(connection.closed().await, CloseReason::SlowConsumer);
    }

    #[tokio::test]
    async fn a_server_close_is_echoed() {
        let (provider, server) = serve(|mut socket| async move {
            let close = CloseFrame { code: CloseCode::from(4008), reason: "slow consumer".into() };
            socket.close(Some(close)).await.expect("a close");
            let echo = socket.next().await.expect("a frame").expect("a read");
            let Message::Close(Some(echo)) = echo else {
                panic!("expected a close echo, got {echo:?}");
            };
            assert_eq!(u16::from(echo.code), 4008);
        })
        .await;

        let connection = provider.connect().await.expect("a socket");

        assert_eq!(connection.closed().await, CloseReason::SlowConsumer);
        server.await.expect("the server reads the echo");
    }

    #[tokio::test]
    async fn closing_the_connection_ends_its_subscriptions() {
        let (provider, server) = serve(|mut socket| async move {
            ack(&mut socket).await;
            let frame = socket.next().await.expect("a frame").expect("a read");
            let Message::Close(Some(close)) = frame else {
                panic!("expected a close, got {frame:?}");
            };
            assert_eq!(close.code, CloseCode::Normal);
        })
        .await;

        let connection = provider.connect().await.expect("a socket");
        let mut candles = connection.subscribe(&candles()).await.expect("an ack");
        connection.close().await;

        let end = candles.next().await.expect("an item").expect_err("a close");
        assert!(matches!(end, MetastreamsError::Closed(CloseReason::Client)));
        server.await.expect("the server reads the close");
    }

    // The paused clock jumps ahead of a TCP frame in flight. An in-memory socket wakes its reader at once.
    #[tokio::test(start_paused = true)]
    async fn a_server_that_falls_silent_is_lost_one_idle_timeout_after_its_last_frame() {
        let (client, server) = tokio::io::duplex(1 << 16);
        let mut server = WebSocketStream::from_raw_socket(server, Role::Server, None).await;
        let client = WebSocketStream::from_raw_socket(client, Role::Client, None).await;
        let lull = Duration::from_millis(500);
        tokio::spawn(async move {
            server.next().await.expect("a ping").expect("a read");
            // The last frame lands between two ping ticks.
            tokio::time::sleep(lull).await;
            let pong = serde_json::to_string(&ServerMessage::Pong).expect("a server frame");
            server.send(Message::text(pong)).await.expect("a write");
            while let Some(Ok(_)) = server.next().await {}
        });
        let start = Instant::now();

        let reason = StreamConnection::spawn(client).closed().await;

        assert_eq!(reason, CloseReason::Lost(LostCause::IdleTimeout));
        let lost = start.elapsed();
        let deadline = PING_INTERVAL + lull + IDLE_TIMEOUT;
        assert!(lost >= deadline, "the pong did not move the deadline: {lost:?}");
        // The timer rounds up to its next millisecond. A check on the ping tick waits a tick more.
        assert!(lost < deadline + Duration::from_secs(1), "lost after {lost:?}");
    }

    #[tokio::test]
    async fn a_server_that_answers_each_ping_stays_open_past_the_idle_timeout() {
        let (provider, _server) = serve(|mut socket| async move {
            while let Some(Ok(Message::Text(text))) = socket.next().await {
                if matches!(serde_json::from_str(text.as_str()), Ok(ClientMessage::Ping)) {
                    write(&mut socket, &ServerMessage::Pong).await;
                }
            }
        })
        .await;
        let connection = provider.connect().await.expect("a socket");

        tokio::time::pause();
        let open = timeout(IDLE_TIMEOUT * 3, connection.closed()).await;

        assert!(open.is_err(), "closed with {open:?}");
    }

    #[tokio::test]
    async fn a_revoked_key_is_not_transient() {
        let (provider, _server) = serve(|mut socket| async move {
            ack(&mut socket).await;
            let close = CloseFrame { code: CloseCode::Policy, reason: "key revoked".into() };
            socket.close(Some(close)).await.expect("a close");
            drain(socket).await;
        })
        .await;

        let connection = provider.connect().await.expect("a socket");
        let mut candles = connection.subscribe(&candles()).await.expect("an ack");

        let end = candles.next().await.expect("an item").expect_err("a close");
        assert!(matches!(&end, MetastreamsError::Closed(CloseReason::KeyRevoked)));
        assert!(!end.is_transient());
    }

    #[tokio::test]
    async fn an_organization_out_of_credits_is_not_transient() {
        let (provider, _server) = serve(|mut socket| async move {
            ack(&mut socket).await;
            let close =
                CloseFrame { code: CloseCode::from(4402), reason: "credits exhausted".into() };
            socket.close(Some(close)).await.expect("a close");
            drain(socket).await;
        })
        .await;

        let connection = provider.connect().await.expect("a socket");
        let mut candles = connection.subscribe(&candles()).await.expect("an ack");

        let end = candles.next().await.expect("an item").expect_err("a close");
        assert!(matches!(&end, MetastreamsError::Closed(CloseReason::CreditsExhausted)));
        assert!(!end.is_transient());
    }

    #[tokio::test]
    async fn dropping_a_subscription_unsubscribes_it() {
        let (provider, server) = serve(|mut socket| async move {
            let id = ack(&mut socket).await;
            let unsubscribe = read(&mut socket).await;
            assert_eq!(
                unsubscribe,
                ClientMessage::Unsubscribe {
                    channel: "spot.candles".to_owned(),
                    subscription_id: Some(id),
                }
            );
        })
        .await;

        let connection = provider.connect().await.expect("a socket");
        drop(connection.subscribe(&candles()).await.expect("an ack"));

        server.await.expect("the server reads the unsubscribe");
    }

    #[tokio::test]
    async fn a_subscribe_dropped_after_its_ack_unsubscribes() {
        let (acked, on_ack) = oneshot::channel();
        let (provider, server) = serve(|mut socket| async move {
            let id = ack(&mut socket).await;
            acked.send(()).expect("the test waits");
            let unsubscribe = read(&mut socket).await;
            assert_eq!(
                unsubscribe,
                ClientMessage::Unsubscribe {
                    channel: "spot.candles".to_owned(),
                    subscription_id: Some(id),
                }
            );
        })
        .await;

        let connection = provider.connect().await.expect("a socket");
        let filter = candles();
        let mut subscribe = Box::pin(connection.subscribe(&filter));
        assert!(poll!(&mut subscribe).is_pending(), "the subscribe waits for its ack");
        on_ack.await.expect("an ack");
        // Leave the driver time to hand the ack to a caller that never reads it.
        tokio::time::sleep(Duration::from_millis(50)).await;
        drop(subscribe);

        server.await.expect("the server reads the unsubscribe");
    }

    #[tokio::test]
    async fn a_socket_cut_mid_handshake_is_transient() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
        let address = listener.local_addr().expect("an address");
        tokio::spawn(async move {
            let (mut tcp, _) = listener.accept().await.expect("a client");
            // Read the whole upgrade, so the hang-up is a clean end rather than a reset.
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut chunk = [0; 1024];
                let read = tcp.read(&mut chunk).await.expect("a read");
                request.extend_from_slice(&chunk[..read]);
            }
        });
        let provider = MetastreamsProvider::new_with_base_url(KEY, &format!("http://{address}"))
            .expect("provider");

        let error = provider.connect().await.expect_err("a hang-up");

        assert!(matches!(error, MetastreamsError::WebSocket(_)), "{error:?}");
        assert!(error.is_transient());
    }

    #[tokio::test]
    async fn a_refused_handshake_reads_as_the_rest_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/stream"))
            .respond_with(ResponseTemplate::new(401).set_body_json(
                json!({ "error": { "code": "UNAUTHORIZED", "message": "Invalid API key." } }),
            ))
            .mount(&server)
            .await;

        let provider =
            MetastreamsProvider::new_with_base_url(KEY, &server.uri()).expect("provider");
        let error = provider.connect().await.expect_err("a refusal");

        assert!(matches!(
            &error,
            MetastreamsError::Api { error: detail, .. } if detail.code == ErrorCode::Unauthorized
        ));
        assert!(!error.is_transient());
    }
}
