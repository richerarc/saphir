use std::time::SystemTime;

pub enum EntityTag {
    Strong(String),
    Weak(String),
}

impl EntityTag {
    pub fn new(is_weak: bool, tag: &str) -> Self {
        if is_weak {
            EntityTag::Weak(tag.to_string())
        } else {
            EntityTag::Strong(tag.to_string())
        }
    }

    pub fn parse(tag: &str) -> Self {
        let mut is_weak = false;
        let parsed_tag = {
            if tag.starts_with("W/") {
                is_weak = true;
                tag.trim_start_matches("W/\"").trim_end_matches("\"")
            } else {
                tag.trim_start_matches("\"").trim_end_matches("\"")
            }
        };

        if is_weak {
            EntityTag::Weak(parsed_tag.to_string())
        } else {
            EntityTag::Strong(parsed_tag.to_string())
        }
    }

    pub fn get_tag(&self) -> String {
        match self {
            EntityTag::Strong(tag) => format!("\"{}\"", tag),
            EntityTag::Weak(tag) => format!("W/\"{}\"", tag),
        }
    }

    fn is_weak(&self) -> bool {
        match self {
            EntityTag::Weak(_) => true,
            _ => false,
        }
    }

    pub fn weak_eq(&self, other: EntityTag) -> bool {
        self.as_ref() == other.as_ref()
    }

    pub fn strong_eq(&self, other: EntityTag) -> bool {
        !self.is_weak() && !other.is_weak() && self.as_ref() == other.as_ref()
    }
}

impl AsRef<str> for EntityTag {
    fn as_ref(&self) -> &str {
        match self {
            EntityTag::Strong(str) => str.as_str(),
            EntityTag::Weak(str) => str.as_str(),
        }
    }
}

pub trait SystemTimeExt {
    fn timestamp(&self) -> u64;
}

impl SystemTimeExt for SystemTime {
    /// Convert `SystemTime` to timestamp in seconds.
    fn timestamp(&self) -> u64 {
        self.duration_since(::std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
    }
}
