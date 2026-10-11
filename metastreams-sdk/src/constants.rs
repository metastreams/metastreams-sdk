//! Where the API lives, how long one request may take, and how the stream socket keeps time.

use std::time::Duration;

/// The production API origin.
pub const BASE_URL: &str = "https://api.metastreams.io";

/// Deadline for one REST request: connect, transfer and body read.
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Deadline for the TCP and TLS connect, and for the stream socket's whole handshake.
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// How often an idle stream socket sends `ping`; the server never pings first.
pub(crate) const PING_INTERVAL: Duration = Duration::from_secs(15);

/// How long a stream socket may stay silent before it counts as lost.
pub(crate) const IDLE_TIMEOUT: Duration = Duration::from_secs(45);

/// How long the closing handshake may take before the SDK drops the socket.
pub(crate) const CLOSE_GRACE: Duration = Duration::from_secs(5);

/// Subscribes and unsubscribes one stream socket may hold unsent.
pub(crate) const COMMAND_BUFFER: usize = 256;

/// Updates and exits one subscription may hold unread before the SDK closes it.
pub(crate) const SUBSCRIPTION_BUFFER: usize = 1024;
