use crate::infrai_queue::{InfraiError, QueueClient, QueuedMessage};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_DELIVERY_ATTEMPTS: u8 = 3;
pub const FAILED_DELIVERY_QUEUE: &str = "course-delivery-failures";
const EDUCATOR_DEAD_LETTER_QUEUE: &str = "educator-dead-letter-reports";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CourseDeliveryJob {
    pub job_id: String,
    pub course_id: String,
    pub learner_id: String,
    pub deadline_epoch: u64,
    pub attempt: u8,
}

#[derive(Debug, PartialEq, Eq)]
pub enum FailureDecision {
    Retry,
    DeadLetter,
}

#[derive(Debug, Serialize)]
struct EducatorDeadLetter<'a> {
    kind: &'static str,
    job_id: &'a str,
    course_id: &'a str,
    learner_id: &'a str,
    deadline_epoch: u64,
    attempts: u8,
    failure: &'a str,
}

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error(transparent)]
    Queue(#[from] InfraiError),
    #[error("message {message_id} has invalid course delivery data: {source}")]
    InvalidJob {
        message_id: String,
        source: serde_json::Error,
    },
}

pub fn classify_failure(job: &CourseDeliveryJob, now_epoch: u64) -> FailureDecision {
    if job.attempt >= MAX_DELIVERY_ATTEMPTS || now_epoch >= job.deadline_epoch {
        FailureDecision::DeadLetter
    } else {
        FailureDecision::Retry
    }
}

pub async fn handle_failed_delivery(
    infrai: &QueueClient,
    message: QueuedMessage,
    now_epoch: u64,
    failure: &str,
) -> Result<FailureDecision, WorkerError> {
    let job: CourseDeliveryJob =
        serde_json::from_value(message.payload).map_err(|source| WorkerError::InvalidJob {
            message_id: message.message_id.clone(),
            source,
        })?;
    let decision = classify_failure(&job, now_epoch);

    if decision == FailureDecision::DeadLetter {
        let report = EducatorDeadLetter {
            kind: "course_delivery_dead_letter",
            job_id: &job.job_id,
            course_id: &job.course_id,
            learner_id: &job.learner_id,
            deadline_epoch: job.deadline_epoch,
            attempts: job.attempt,
            failure,
        };
        let key = format!("dead-letter:{}", job.job_id);
        infrai
            .publish(EDUCATOR_DEAD_LETTER_QUEUE, &report, &key)
            .await?;
        infrai
            .ack(FAILED_DELIVERY_QUEUE, &message.message_id)
            .await?;
    }

    Ok(decision)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(attempt: u8, deadline_epoch: u64) -> CourseDeliveryJob {
        CourseDeliveryJob {
            job_id: "delivery-42".into(),
            course_id: "rust-async".into(),
            learner_id: "learner-7".into(),
            deadline_epoch,
            attempt,
        }
    }

    #[test]
    fn expired_learner_delivery_goes_to_educator_report() {
        assert_eq!(
            classify_failure(&job(1, 1_700_000_000), 1_700_000_001),
            FailureDecision::DeadLetter
        );
        assert_eq!(
            classify_failure(&job(1, 1_700_000_100), 1_700_000_001),
            FailureDecision::Retry
        );
        assert_eq!(
            classify_failure(&job(3, 1_700_000_100), 1_700_000_001),
            FailureDecision::DeadLetter
        );
    }
}
