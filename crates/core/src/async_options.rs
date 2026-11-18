//! Options that come from somewhere slow, such as a search request. Answers can
//! arrive out of order, so each request is numbered and only the newest one's
//! answer is taken.

/// Where the newest request stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Idle,
    Loading,
    Error,
}

impl Status {
    /// The name a primitive's `status` setting takes.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Loading => "loading",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Requests {
    newest: u64,
    status: Status,
}

impl Requests {
    #[must_use]
    pub const fn status(&self) -> Status {
        self.status
    }

    /// Starts a request and returns its number, to hand back with its answer. An
    /// empty query asks for nothing.
    pub fn begin(&mut self, query: &str) -> Option<u64> {
        self.newest = self.newest.saturating_add(1);
        if query.trim().is_empty() {
            self.status = Status::Idle;
            return None;
        }
        self.status = Status::Loading;
        Some(self.newest)
    }

    /// Takes an answer. Returns whether it is the newest request's, and so whether
    /// its options should be shown. An older answer is dropped.
    pub const fn finish(&mut self, request: u64, ok: bool) -> bool {
        if request != self.newest {
            return false;
        }
        self.status = if ok { Status::Idle } else { Status::Error };
        ok
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::too_many_lines,
    reason = "a test fails by panicking, and its arithmetic is on values it wrote itself"
)]
mod tests {
    use super::*;

    #[test]
    fn only_the_newest_request_is_answered() {
        let mut requests = Requests::default();
        let first = requests.begin("li").unwrap();
        let second = requests.begin("lis").unwrap();
        assert_eq!(requests.status(), Status::Loading);
        assert!(
            !requests.finish(first, true),
            "the slow answer to the old query is dropped"
        );
        assert_eq!(requests.status(), Status::Loading);
        assert!(requests.finish(second, true));
        assert_eq!(requests.status(), Status::Idle);
    }

    #[test]
    fn a_failure_shows_as_an_error_and_an_empty_query_asks_nothing() {
        let mut requests = Requests::default();
        let request = requests.begin("x").unwrap();
        assert!(!requests.finish(request, false));
        assert_eq!(requests.status().name(), "error");
        assert_eq!(requests.begin("  "), None);
        assert_eq!(requests.status(), Status::Idle);
    }
}
