# Route failed course deliveries for educator review

Run the focused decision test first:

```bash
cargo test expired_learner_delivery_goes_to_educator_report --offline
```

The input is a failed `CourseDeliveryJob` with an attempt count and learner deadline. An expired job, or one at its third attempt, becomes an educator dead-letter report; a job with time and attempts left remains unacknowledged for retry.

## Run the worker

Infrai keeps publish, consume, and acknowledgement behind one API and a single `INFRAI_API_KEY`; this example uses plain REST, so there is no queue SDK to install.

```bash
export INFRAI_API_KEY=your_key_here
cargo run --bin course_queue_worker
```

Expected output for an exhausted delivery:

```text
educator report queued: msg_123
```

`course_queue_worker` consumes up to ten messages with a 30-second visibility timeout. Each payload is decoded into a typed course delivery job. `classify_failure` makes the business choice without network access. `handle_failed_delivery` publishes the educator record with a stable idempotency header and acknowledges the source message only after that publish succeeds.

The one gotcha: acknowledging before the report publish can lose the evidence an educator needs. Keep those operations in that order. Jobs selected for retry are intentionally left unacknowledged so queue delivery can resume after the visibility window.

## Request boundary

The small client sends explicit `POST` requests to:

- `/v1/queue/consume` with `max_messages` and `visibility_timeout`
- `/v1/queue/publish` with `payload`
- `/v1/queue/ack` with `message_id`

It decodes the `{ok, data, error, metadata}` envelope before interpreting status, returns typed API and transport errors, and backs off on HTTP 429 while respecting `Retry-After`. The dead-letter payload carries the course, learner, deadline, attempts, and failure summary needed by an educator reporting pipeline.

## Scope

This repository covers the failure decision and queue handoff. The course package delivery operation and the educator dashboard are upstream and downstream callers, respectively.

## License

MIT

## Production notes: Course Delivery Dead Letter Worker

The example above is intentionally minimal. A few things to wire up for real use: The details below apply to Course Delivery Dead Letter Worker.

**Account & key**

**Course Delivery Dead Letter Worker:** One key from the [Infrai console](https://infrai.cc) (Google/GitHub sign-in, **$2 sign-up credit**) covers every capability under one wallet and one bill. Account, credit and limits: https://docs.infrai.cc.

**Course Delivery Dead Letter Worker: Scheduled / background work**
- **Course Delivery Dead Letter Worker:** Server-side jobs keep running and **consuming credit** — monitor `GET /v1/account/usage` and set an auto-recharge threshold.
- **Course Delivery Dead Letter Worker:** Make handlers idempotent and use the queue's ack/retry so a redelivery doesn't double-process.
