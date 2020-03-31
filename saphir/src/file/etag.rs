use std::time::SystemTime;

pub struct EntityTag {
    tag: Tag,
}

enum Tag {
    Strong(String),
    Weak(String),
}

impl EntityTag {
    pub fn new(is_weak: bool, tag: &str) -> Self {
        if is_weak {
            EntityTag {
                tag: Tag::Weak(tag.to_string()),
            }
        } else {
            EntityTag {
                tag: Tag::Strong(tag.to_string()),
            }
        }
    }

    pub fn get_tag(&self) -> String {
        match &self.tag {
            Tag::Strong(tag) => format!("\"{}\"", tag),
            Tag::Weak(tag) => format!("W/\"{}\"", tag),
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
