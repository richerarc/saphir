use crate::{request::Request, response::Response, router::Router};

#[cfg(feature = "operation")]
pub static OPERATION_ID_HEADER: &str = "Operation-Id";

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

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum RouteId {
    Id(u64),
    Error(u16),
}

impl RouteId {
    pub(crate) fn new(id: u64) -> Self {
        RouteId::Id(id)
    }
}

impl Default for RouteId {
    fn default() -> Self {
        RouteId::Error(404)
    }
}

/// MetaData of the resolved request handler
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct HandlerMetadata {
    pub route_id: RouteId,
    pub name: Option<&'static str>,
}

impl HandlerMetadata {
    pub(crate) fn not_found() -> Self {
        HandlerMetadata {
            route_id: Default::default(),
            name: None,
        }
    }

    pub(crate) fn not_allowed() -> Self {
        HandlerMetadata {
            route_id: RouteId::Error(405),
            name: None,
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
    pub metadata: HandlerMetadata,
    pub(crate) router: Option<Router>,
}

impl HttpContext {
    pub(crate) fn new(request: Request, router: Router, metadata: HandlerMetadata) -> Self {
        #[cfg(not(feature = "operation"))]
        {
            let state = State::Before(Box::new(request));
            let router = Some(router);
            HttpContext { state, metadata, router }
        }

        #[cfg(feature = "operation")]
        {
            use std::str::FromStr;
            let mut request = request;
            let operation_id = request
                .headers()
                .get(OPERATION_ID_HEADER)
                .and_then(|h| h.to_str().ok())
                .and_then(|op_id_str| operation::OperationId::from_str(op_id_str).ok())
                .unwrap_or_else(operation::OperationId::new);
            *request.operation_id_mut() = operation_id;
            let state = State::Before(Box::new(request));
            let router = Some(router);
            HttpContext {
                state,
                router,
                operation_id,
                metadata,
            }
        }
    }

    pub fn clone_with_empty_state(&self) -> Self {
        HttpContext {
            state: State::Empty,
            router: self.router.clone(),
            metadata: self.metadata.clone(),
            #[cfg(feature = "operation")]
            operation_id: self.operation_id.clone(),
        }
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
    use serde::{de::Visitor, Deserialize, Deserializer, Serialize, Serializer};
    use std::{
        fmt::{Debug, Display, Formatter},
        str::FromStr,
    };
    use uuid::Uuid;

    /// Represent a single operation from a incoming request until a response is
    /// produced
    #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd, Default)]
    pub struct OperationId(Uuid);

    impl OperationId {
        pub fn new() -> OperationId {
            OperationId(Uuid::new_v4())
        }

        pub fn to_u128(&self) -> u128 {
            self.0.as_u128()
        }
    }

    impl Display for OperationId {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            std::fmt::Display::fmt(&self.0.to_hyphenated_ref(), f)
        }
    }

    impl Debug for OperationId {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            std::fmt::Display::fmt(&self.0.to_hyphenated_ref(), f)
        }
    }

    impl FromStr for OperationId {
        type Err = uuid::Error;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            Uuid::parse_str(s).map(OperationId)
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
}
