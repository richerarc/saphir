use crate::{request::Request, router::Router};
use std::sync::atomic::AtomicU64;

const SERVER_ID_OFFSET: usize = 0;
const TIMESTAMP_OFFSET: usize = 4;
const OPERATION_ID_OFFSET: usize = 10;
const OPERATION_ID_MAX: usize = 16;
static OP_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Context representing the relationship between a request and a response
/// This structure only appears inside Middleware since the act before and after
/// the request
///
/// There is no guaranty the the request nor the response will be set at any
/// given time, since they could be moved out by a badly implemented middleware
pub struct HttpContext<B> {
    /// The incoming request before it is handled by the router
    pub request: Request<B>,
    pub(crate) router: Router,
}

impl<B> HttpContext<B> {
    pub(crate) fn new(request: Request<B>, router: Router) -> Self {
        HttpContext { request, router }
    }
}

pub struct OperationId {
    bytes: [u8; 16],
}

impl OperationId {
    pub fn new(server_id: u32) -> OperationId {
        use std::time::{SystemTime, UNIX_EPOCH};
        let t_s = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("UNIX EPOCH should be in the past")
            .as_secs();

        let count = OP_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let mut bytes = [0u8; 16];
        buf[SERVER_ID_OFFSET..TIMESTAMP_OFFSET].clone_from_slice(&server_id.to_le_bytes());
        buf[TIMESTAMP_OFFSET..OPERATION_ID_OFFSET].clone_from_slice(&t_s.to_le_bytes()[0..6]);
        buf[OPERATION_ID_OFFSET..OPERATION_ID_MAX].clone_from_slice(&count.to_le_bytes()[0..6]);

        OperationId { bytes }
    }

    pub fn to_string(&self) -> String {
        let mut s = String::with_capacity(34);
    }

    fn encode<'a>(&self) -> &'a mut str {
        let len = if hyphens { 36 } else { 32 };

        {
            let buffer = &mut full_buffer[start..start + len];
            let bytes = uuid.as_bytes();

            let hex = if upper { &UPPER } else { &LOWER };

            for group in 0..5 {
                // If we're writing hyphens, we need to shift the output
                // location along by how many of them have been written
                // before this point. That's exactly the (0-indexed) group
                // number.
                let hyphens_before = if hyphens { group } else { 0 };
                for idx in BYTE_POSITIONS[group]..BYTE_POSITIONS[group + 1] {
                    let b = bytes[idx];
                    let out_idx = hyphens_before + 2 * idx;

                    buffer[out_idx] = hex[(b >> 4) as usize];
                    buffer[out_idx + 1] = hex[(b & 0b1111) as usize];
                }

                if group != 4 && hyphens {
                    buffer[HYPHEN_POSITIONS[group]] = b'-';
                }
            }
        }

        str::from_utf8_mut(&mut full_buffer[..start + len]).expect("found non-ASCII output characters while encoding a UUID")
    }
}
