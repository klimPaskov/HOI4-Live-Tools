use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Error {
    pub code: &'static str,
    pub message: String,
}
pub type Result<T> = std::result::Result<T, Error>;
impl Error {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    /// Preserve the diagnostic cause without weakening the post-input category.
    /// Keep one bounded line for durable navigation reports; this grants no retry.
    pub(crate) fn after_possible_input(context: &'static str, cause: Error) -> Self {
        let mut message = format!("{context}; cause={}: ", cause.code);
        for c in cause.message.chars().filter(|c| !c.is_control()) {
            if message.len() + c.len_utf8() > 512 {
                break;
            }
            message.push(c);
        }
        Self::new("action_uncertain", message)
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::new(
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                "permission_denied"
            } else {
                "io_error"
            },
            e.to_string(),
        )
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn possible_input_keeps_cause_bounded_but_never_its_retryable_category() {
        for code in [
            "torn_read",
            "permission_denied",
            "ownership_changed",
            "io_error",
        ] {
            let e = Error::after_possible_input(
                "Own readback unavailable; never replay",
                Error::new(code, "field\n".to_owned() + &"é".repeat(600)),
            );
            assert_eq!(e.code, "action_uncertain");
            assert!(e.message.contains(&format!("cause={code}: field")));
            assert!(e.message.len() <= 512);
            assert!(!e.message.chars().any(char::is_control));
        }
    }
}
