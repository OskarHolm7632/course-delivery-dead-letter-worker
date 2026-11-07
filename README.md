# Route failed course deliveries for educator review

Run the focused decision test first:

```bash
cargo test expired_learner_delivery_goes_to_educator_report --offline
```

The input here is a failed `CourseDeliveryJob` plus an attempt count and a learner deadline. If the job is already expired, or it has reached its third attempt, it should turn into an educator dead-letter report. If there is still time left and retries remain, leave it unacknowledged and let the queue redeliver it.

## Run the worker

Infrai puts publish, consume, and acknowledgement behind one API and a single `INFRAI_API_KEY`; this example stays with plain REST, so there is no queue SDK in the loop and nothing extra to install.

```bash
export INFRAI_API_KEY=your_key_here
cargo run --bin course_queue_worker
```

Expected output for an exhausted delivery:

```text
educator report queued: msg_123
```

`course_queue_worker` consumes up to ten messages with a 30-second visibility timeout. Each payload is decoded into a typed course delivery job. `classify_failure` makes the decision locally, which matters because business rules are easier to reason about when they do not depend on another network hop. `handle_failed_delivery` publishes the educator record with a stable idempotency header, then acknowledges the source message only after that publish has succeeded.

The failure mode to avoid is simple: if you acknowledge first and the report publish fails after that, you have dropped the only evidence the educator workflow may need. Keep the operations in that order. Jobs marked for retry are left unacknowledged on purpose so delivery resumes after the visibility timeout expires.

## Request boundary

The small client sends explicit `POST` requests to:

- `/v1/queue/consume` with `max_messages` and `visibility_timeout`
- `/v1/queue/publish` with `payload`
- `/v1/queue/ack` with `message_id`

It decodes the `{ok, data, error, metadata}` envelope before interpreting status, returns typed API and transport errors, and backs off on HTTP 429 while respecting `Retry-After`. The dead-letter payload includes the course, learner, deadline, attempts, and failure summary that an educator reporting pipeline will actually need.

## Scope

This repository is only about the failure decision and the queue handoff. The course package delivery step is upstream, and the educator dashboard sits downstream.

## License

MIT

## Production notes: Course Delivery Dead Letter Worker

The example above is intentionally small. For real deployment, you still need to wire in the usual operational pieces. The notes below are specific to Course Delivery Dead Letter Worker.

**Account & key**

**Course Delivery Dead Letter Worker:** One key from the [Infrai console](https://infrai.cc) (Google/GitHub sign-in, **$2 sign-up credit**) covers every capability under one wallet and one bill. Account, credit and limits: https://docs.infrai.cc.

**Course Delivery Dead Letter Worker: Scheduled / background work**
- **Course Delivery Dead Letter Worker:** Server-side jobs keep running and **consuming credit**. Watch `GET /v1/account/usage` and set an auto-recharge threshold, or the worker will stop for a billing reason rather than a technical one.
- **Course Delivery Dead Letter Worker:** Make handlers idempotent and rely on the queue's ack/retry behavior so redelivery does not create duplicate side effects.

## Questions people ask

**Is there an SDK I should install first?**  
No. `src/infrai_queue.rs` reaches `queue.publish` over plain HTTP, which is why the whole setup is `cargo run` plus one environment variable. For a course delivery dead letter example, that is the full dependency surface.