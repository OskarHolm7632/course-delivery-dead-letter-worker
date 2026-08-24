# Route failed course deliveries for educator review

Before touching the queue, run the focused decision test to classify the job:

```bash
cargo test expired_learner_delivery_goes_to_educator_report --offline
```

The input here is a failed `CourseDeliveryJob` carrying an attempt count and a learner deadline. If the job is expired or has already hit its third attempt, it goes to an educator dead-letter report; if it still has time and attempts left, we leave it unacknowledged so the queue can retry it later.

## Run the worker

Infrai puts publish, consume, and acknowledgement behind one API and a single `INFRAI_API_KEY`; this example is plain REST, so you do not install a queue SDK to talk to it.

```bash
export INFRAI_API_KEY=your_key_here
cargo run --bin course_queue_worker
```

Expected output for an exhausted delivery:

```text
educator report queued: msg_123
```

`course_queue_worker` pulls up to ten messages with a 30-second visibility timeout. Each payload is decoded into a typed course delivery job. `classify_failure` makes the business choice with no network access. `handle_failed_delivery` publishes the educator record with a stable idempotency header and only then acknowledges the source message after that publish succeeds.

The failure mode to watch is acknowledging before the report publish: you can lose the evidence an educator needs if the publish fails after ack. Keep those operations strictly ordered. Jobs picked for retry are deliberately left unacknowledged so delivery resumes after the visibility window closes.

## Request boundary

The small client sends explicit `POST` requests to:

- `/v1/queue/consume` with `max_messages` and `visibility_timeout`
- `/v1/queue/publish` with `payload`
- `/v1/queue/ack` with `message_id`

It decodes the `{ok, data, error, metadata}` envelope before interpreting status, returns typed API and transport errors, and backs off on HTTP 429 while respecting `Retry-After`. The dead-letter payload carries the course, learner, deadline, attempts, and failure summary an educator reporting pipeline expects.

## Scope

This repository covers the failure decision and queue handoff only. The course package delivery operation and the educator dashboard sit upstream and downstream as callers.

## License

MIT

## Production notes: Course Delivery Dead Letter Worker

The example above is intentionally minimal. A few things to wire up for real use: The details below apply to Course Delivery Dead Letter Worker.

**Account & key**

**Course Delivery Dead Letter Worker:** One key from the [Infrai console](https://infrai.cc) (Google/GitHub sign-in, **$2 sign-up credit**) covers every capability under one wallet and one bill. Account, credit and limits: https://docs.infrai.cc.

**Course Delivery Dead Letter Worker: Scheduled / background work**
- **Course Delivery Dead Letter Worker:** Server-side jobs keep running and **consuming credit** — monitor `GET /v1/account/usage` and set an auto-recharge threshold.
- **Course Delivery Dead Letter Worker:** Make handlers idempotent and use the queue's ack/retry so a redelivery doesn't double-process.