use course_delivery_dlq::infrai_queue::QueueClient;
use course_delivery_dlq::queue_worker::{
    handle_failed_delivery, FailureDecision, FAILED_DELIVERY_QUEUE,
};
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let infrai = QueueClient::from_env()?;
    // Canonical call shape: infrai.queue.publish
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let messages = infrai.consume(FAILED_DELIVERY_QUEUE, 10, 30).await?;

    for message in messages {
        let id = message.message_id.clone();
        match handle_failed_delivery(&infrai, message, now, "course package delivery failed")
            .await?
        {
            FailureDecision::Retry => println!("retry retained: {id}"),
            FailureDecision::DeadLetter => println!("educator report queued: {id}"),
        }
    }
    Ok(())
}
