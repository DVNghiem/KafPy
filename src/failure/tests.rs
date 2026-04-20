#[cfg(test)]
mod tests {
    use crate::failure::{
        FailureCategory, FailureReason, TerminalKind, reason::{NonRetryableKind, RetryableKind},
    };

    #[test]
    fn failure_reason_category_retryable() {
        let reason = FailureReason::Retryable(RetryableKind::NetworkTimeout);
        assert_eq!(reason.category(), FailureCategory::Retryable);
    }

    #[test]
    fn failure_reason_category_terminal() {
        let reason = FailureReason::Terminal(TerminalKind::PoisonMessage);
        assert_eq!(reason.category(), FailureCategory::Terminal);
    }

    #[test]
    fn failure_reason_category_non_retryable() {
        let reason = FailureReason::NonRetryable(NonRetryableKind::ValidationError);
        assert_eq!(reason.category(), FailureCategory::NonRetryable);
    }

    #[test]
    fn failure_reason_display() {
        let reason = FailureReason::Retryable(RetryableKind::NetworkTimeout);
        assert_eq!(format!("{}", reason), "Retryable: network timeout");
    }

    #[test]
    fn failure_reason_eq() {
        let reason1 = FailureReason::Terminal(TerminalKind::DeserializationFailed);
        let reason2 = FailureReason::Terminal(TerminalKind::DeserializationFailed);
        assert_eq!(reason1, reason2);
    }
}
