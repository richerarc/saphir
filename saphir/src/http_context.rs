use crate::{request::Request, response::Response, router::Router};

/// State of the Http context. It represent whether the context is used
/// `Before(..)` or `After(..)` calling the handler responsible of generating a
/// responder. Empty will be the state of a context when the request is being
/// processed by the handler, or when its original state has been moved by using
/// take & take unchecked methods
pub enum State {
    Before(Box<Request>),
    After(Box<Response>),
    Empty,
}

impl Default for State {
    fn default() -> Self {
        State::Empty
    }
}

impl State {
    /// Take the current context leaving `State::Empty` behind
    pub fn take(&mut self) -> Self {
        std::mem::take(self)
    }

    /// Take the current request leaving `State::Empty` behind
    /// Returns `Some(Request)` if the state was `Before` or `None` if it was
    /// something else
    pub fn take_request(&mut self) -> Option<Request> {
        match std::mem::take(self) {
            State::Before(r) => Some(*r),
            _ => None,
        }
    }

    /// Take the current request leaving `State::Empty` behind
    ///
    /// # Panics
    /// Panics if the state is not `Before`
    pub fn take_request_unchecked(&mut self) -> Request {
        match std::mem::take(self) {
            State::Before(r) => *r,
            _ => panic!("State::take_request_unchecked should be called only before the handler & when it is ensured that the request wasn't moved"),
        }
    }

    /// Take the current response leaving `State::Empty` behind
    /// Returns `Some(Response)` if the state was `After` or `None` if it was
    /// something else
    pub fn take_response(&mut self) -> Option<Response> {
        match std::mem::take(self) {
            State::After(r) => Some(*r),
            _ => None,
        }
    }

    /// Take the current response leaving `State::Empty` behind
    ///
    /// # Panics
    /// Panics if the state is not `After`
    pub fn take_response_unchecked(&mut self) -> Response {
        match std::mem::take(self) {
            State::After(r) => *r,
            _ => panic!("State::take_response_unchecked should be called only after the handler & when it is ensured that the response wasn't moved"),
        }
    }

    /// Returns `Some` of the current request if state if `Before`
    pub fn request(&self) -> Option<&Request> {
        match self {
            State::Before(r) => Some(r),
            _ => None,
        }
    }

    /// Returns `Some` of the current request as a mutable ref if state if
    /// `Before`
    pub fn request_mut(&mut self) -> Option<&Request> {
        match self {
            State::Before(r) => Some(r),
            _ => None,
        }
    }

    /// Returns the current request
    ///
    /// # Panics
    /// Panics if state is not `Before`
    pub fn request_unchecked(&self) -> &Request {
        match self {
            State::Before(r) => r,
            _ => panic!("State::request_unchecked should be called only before the handler & when it is ensured that the request wasn't moved"),
        }
    }

    /// Returns the current request as a mutable ref
    ///
    /// # Panics
    /// panics if state is not `Before`
    pub fn request_unchecked_mut(&mut self) -> &mut Request {
        match self {
            State::Before(r) => r,
            _ => panic!("State::request_unchecked_mut should be called only before the handler & when it is ensured that the request wasn't moved"),
        }
    }

    /// Returns `Some` of the current response if state if `After`
    pub fn response(&self) -> Option<&Response> {
        match self {
            State::After(r) => Some(r),
            _ => None,
        }
    }

    /// Returns `Some` of the current response as a mutable ref if state if
    /// `After`
    pub fn response_mut(&mut self) -> Option<&mut Response> {
        match self {
            State::After(r) => Some(r),
            _ => None,
        }
    }

    /// Returns the current response
    ///
    /// # Panics
    /// Panics if state is not `After`
    pub fn response_unchecked(&self) -> &Response {
        match self {
            State::After(r) => r,
            _ => panic!("State::response_unchecked should be called only before the handler & when it is ensured that the request wasn't moved"),
        }
    }

    /// Returns the current response as a mutable ref
    ///
    /// # Panics
    /// Panics if state is not `After`
    pub fn response_unchecked_mut(&mut self) -> &mut Response {
        match self {
            State::After(r) => r,
            _ => panic!("State::response_unchecked_mut should be called only before the handler & when it is ensured that the request wasn't moved"),
        }
    }
}

/// Context representing the relationship between a request and a response
/// This structure only appears inside Middleware since the act before and after
/// the request
///
/// There is no guaranty the the request nor the response will be set at any
/// given time, since they could be moved out by a badly implemented middleware
pub struct HttpContext {
    /// The incoming request `Before` it is handled by the router
    /// OR
    /// The outgoing response `After` the request was handled by the router
    pub state: State,
    #[cfg(feature = "operation")]
    /// Unique Identifier of the current request->response chain
    pub operation_id: crate::http_context::operation::OperationId,
    pub(crate) router: Option<Router>,
}

impl HttpContext {
    #[cfg(not(feature = "operation"))]
    pub(crate) fn new(request: Request, router: Router) -> Self {
        let state = State::Before(Box::new(request));
        let router = Some(router);
        HttpContext { state, router }
    }

    #[cfg(feature = "operation")]
    pub(crate) fn new(server_id: u32, mut request: Request, router: Router) -> Self {
        let operation_id = crate::http_context::operation::OperationId::new(server_id);
        *request.operation_id_mut() = operation_id.clone();
        let state = State::Before(Box::new(request));
        let router = Some(router);
        HttpContext { state, router, operation_id }
    }

    /// Explicitly set the inner state to `Before` with the given response
    pub fn before(&mut self, request: Request) {
        self.state = State::Before(Box::new(request))
    }

    /// Explicitly set the inner state to `After` with the given response
    pub fn after(&mut self, response: Response) {
        self.state = State::After(Box::new(response))
    }
}

#[cfg(feature = "operation")]
pub mod operation {
    use hex::encode_to_slice;
    use serde::{de::Visitor, Deserialize, Deserializer, Serialize, Serializer};
    use std::{
        fmt::{Display, Formatter},
        str::FromStr,
        sync::atomic::AtomicU64,
    };
    use std::fmt::Debug;
    use nom::lib::std::fmt::Error;

    const SERVER_ID_OFFSET: usize = 0;
    const TIMESTAMP_OFFSET: usize = 4;
    const OPERATION_ID_OFFSET: usize = 10;
    const OPERATION_ID_MAX: usize = 16;
    const OPERATION_ID_STR_LEN: usize = OPERATION_ID_MAX * 2 + 2;
    static OP_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

    /// Represent a single operation from a incoming request until a response is
    /// produced
    #[derive(Clone, PartialOrd, PartialEq)]
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

            Self::with_part(server_id, t_s, count)
        }

        pub fn with_part(server_id: u32, timestamp: u64, count: u64) -> OperationId {
            let mut bytes = [0u8; 16];
            bytes[SERVER_ID_OFFSET..TIMESTAMP_OFFSET].copy_from_slice(&server_id.to_be_bytes());
            bytes[TIMESTAMP_OFFSET..OPERATION_ID_OFFSET].copy_from_slice(&timestamp.to_be_bytes()[2..8]);
            bytes[OPERATION_ID_OFFSET..OPERATION_ID_MAX].copy_from_slice(&count.to_be_bytes()[2..8]);

            OperationId { bytes }
        }

        pub(crate) fn with_bytes(bytes: [u8; 16]) -> OperationId {
            OperationId { bytes }
        }

        pub fn to_u128(&self) -> u128 {
            let mut bytes = [0u8; 16];
            bytes.copy_from_slice(&self.bytes);
            u128::from_le_bytes(bytes)
        }

        pub fn server_id(&self) -> u32 {
            let mut s_id_bytes = [0u8; 4];
            if let Some(b) = self.bytes.get(SERVER_ID_OFFSET..TIMESTAMP_OFFSET) {
                s_id_bytes.copy_from_slice(b)
            }
            u32::from_be_bytes(s_id_bytes)
        }

        pub fn timestamp(&self) -> u64 {
            let mut bytes = [0u8; 8];
            bytes
                .get_mut(2..2 + (OPERATION_ID_OFFSET - TIMESTAMP_OFFSET))
                .and_then(|t_b| self.bytes.get(TIMESTAMP_OFFSET..OPERATION_ID_OFFSET).map(|b| t_b.copy_from_slice(b)));
            u64::from_be_bytes(bytes)
        }

        pub fn count(&self) -> u64 {
            let mut bytes = [0u8; 8];
            bytes
                .get_mut(2..2 + (OPERATION_ID_MAX - OPERATION_ID_OFFSET))
                .and_then(|t_b| self.bytes.get(OPERATION_ID_OFFSET..OPERATION_ID_MAX).map(|b| t_b.copy_from_slice(b)));
            u64::from_be_bytes(bytes)
        }

        fn encode_part(&self, dst: &mut [u8], part: usize) {
            match part {
                0 => {
                    let slice = &self.bytes[SERVER_ID_OFFSET..TIMESTAMP_OFFSET];
                    if dst.len() >= slice.len() * 2 {
                        encode_to_slice(slice, dst).expect("This will always work 0");
                    }
                }
                1 => {
                    let slice = &self.bytes[TIMESTAMP_OFFSET..OPERATION_ID_OFFSET];
                    if dst.len() >= slice.len() * 2 {
                        encode_to_slice(slice, dst).expect("This will always work 1");
                    }
                }
                _ => {
                    let slice = &self.bytes[OPERATION_ID_OFFSET..OPERATION_ID_MAX];
                    if dst.len() >= slice.len() * 2 {
                        encode_to_slice(slice, dst).expect("This will always work 2");
                    }
                }
            }
        }
    }

    impl Display for OperationId {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            let mut vec = vec![0u8; OPERATION_ID_STR_LEN];

            let str = vec
                .get_mut(..)
                .map(|bytes| {
                    if let Some(dst) = bytes.get_mut(SERVER_ID_OFFSET..TIMESTAMP_OFFSET * 2) {
                        self.encode_part(dst, 0)
                    }
                    if let Some(byte) = bytes.get_mut(TIMESTAMP_OFFSET * 2) {
                        *byte = b'-'
                    }
                    if let Some(dst) = bytes.get_mut(TIMESTAMP_OFFSET * 2 + 1..OPERATION_ID_OFFSET * 2 + 1) {
                        self.encode_part(dst, 1)
                    }
                    if let Some(byte) = bytes.get_mut(OPERATION_ID_OFFSET * 2 + 1) {
                        *byte = b'-'
                    }
                    if let Some(dst) = bytes.get_mut(OPERATION_ID_OFFSET * 2 + 2..OPERATION_ID_MAX * 2 + 2) {
                        self.encode_part(dst, 2)
                    }
                })
                .and_then(|_| std::str::from_utf8(vec.as_slice()).ok())
                .expect("Encode should never produce non-ascii chars");

            f.write_str(str)
        }
    }

    impl Debug for OperationId {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("OperationId")
                .field("server_id", &self.server_id())
                .field("timestamp", &self.timestamp())
                .field("operation", &self.count())
                .finish()
        }
    }

    #[derive(Debug)]
    pub enum ParseError {
        MissingSegment,
        InvalidHex(hex::FromHexError),
    }

    impl Display for ParseError {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match self {
                ParseError::MissingSegment => f.write_str("Missing Segment"),
                ParseError::InvalidHex(e) => e.fmt(f),
            }
        }
    }

    impl FromStr for OperationId {
        type Err = ParseError;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            let mut split = s.split('-');
            let mut bytes = [0u8; 16];

            hex::decode_to_slice(
                split.next().ok_or_else(|| ParseError::MissingSegment)?,
                &mut bytes[SERVER_ID_OFFSET..TIMESTAMP_OFFSET],
            )
            .map_err(ParseError::InvalidHex)?;
            hex::decode_to_slice(
                split.next().ok_or_else(|| ParseError::MissingSegment)?,
                &mut bytes[TIMESTAMP_OFFSET..OPERATION_ID_OFFSET],
            )
            .map_err(ParseError::InvalidHex)?;
            hex::decode_to_slice(
                split.next().ok_or_else(|| ParseError::MissingSegment)?,
                &mut bytes[OPERATION_ID_OFFSET..OPERATION_ID_MAX],
            )
            .map_err(ParseError::InvalidHex)?;

            Ok(OperationId { bytes })
        }
    }

    struct OperationIdVisitor;

    impl<'de> Visitor<'de> for OperationIdVisitor {
        type Value = OperationId;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("Invalid operation id")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            v.parse::<OperationId>().map_err(serde::de::Error::custom)
        }

        fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            v.parse::<OperationId>().map_err(serde::de::Error::custom)
        }
    }

    impl<'de> Deserialize<'de> for OperationId {
        fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_str(OperationIdVisitor)
        }
    }

    impl Serialize for OperationId {
        fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
        where
            S: Serializer,
        {
            serializer.serialize_str(self.to_string().as_str())
        }
    }

    #[cfg(test)]
    mod tests {
        use crate::http_context::operation::OperationId;
        use std::str::FromStr;

        #[test]
        pub fn operation_with_part() {
            let op_id = OperationId::with_part(1, 1_530_000_000, 12);

            assert_eq!(op_id.server_id(), 1);
            assert_eq!(op_id.timestamp(), 1_530_000_000);
            assert_eq!(op_id.count(), 12);
            assert_eq!(op_id.to_u128(), 15_950_735_949_419_599_405_423_994_206_418_894_848);

            let str_id = op_id.to_string();
            let mut split = str_id.split('-');

            assert_eq!(split.next(), Some("00000001"));
            assert_eq!(split.next(), Some("00005b31f280"));
            assert_eq!(split.next(), Some("00000000000c"));
            assert_eq!(split.next(), None);
        }

        #[test]
        pub fn operation_from_str() {
            let op_id = OperationId::from_str("00000001-00005b31f280-00000000000c").unwrap();

            assert_eq!(op_id.server_id(), 1);
            assert_eq!(op_id.timestamp(), 1_530_000_000);
            assert_eq!(op_id.count(), 12);
            assert_eq!(op_id.to_u128(), 15_950_735_949_419_599_405_423_994_206_418_894_848);
        }
    }
}
