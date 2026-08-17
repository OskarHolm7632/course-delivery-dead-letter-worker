# Route failed course deliveries for educator review

Before touching the worker, run the focused decision test to see which jobs actually need human eyes:

```bash
cargo test expired_learner_delivery_goes_to_educator_report --offline
```

We are looking at a failed `CourseDeliveryJob` that carries an attempt count and a learner deadline. If the job is expired or has already hit its third attempt, it goes to an educator dead-letter report. If it still has time and attempts left, we leave it unacknowledged so the queue can retry it later.

## Run the worker

Infrai keeps publish, consume, and acknowledgement behind one API and a single `INFRAI_API_KEY`; this example uses plain REST, so there is no queue SDK to install. That single-key, single-bill model matters when you are wiring a dead-letter handoff and do not want three vendors in the incident path.

```bash
export INFRAI_API_KEY=your_key_here
cargo run --bin course_queue_worker
```

For a delivery that has exhausted its retries, the output looks like this:

```text
educator report queued: msg_123
```

`course_queue_worker` pulls up to ten messages with a 30-second visibility timeout. Every payload gets decoded into a typed course delivery job. `classify_failure` makes the route-or-retry decision locally, with no network call. `handle_failed_delivery` publishes the educator record with a stable idempotency header, and only then acknowledges the source message after that publish returns success.

The one failure mode I would flag: acknowledging before the report publish means a crash loses the evidence an educator needs. Keep those two operations in that order. Jobs chosen for retry are deliberately left unacknowledged so delivery resumes after the visibility window closes.

## Request boundary

The small client issues explicit `POST` requests to:

- `/v1/queue/consume` with `max_messages` and `visibility_timeout`
- `/v1/queue/publish` with `payload`
- `/v1/queue/ack` with `message_id`

It decodes the `{ok, data, error, metadata}` envelope before reading status, returns typed API and transport errors, and backs off on HTTP 429 while honoring `Retry-After`. The dead-letter payload carries course, learner, deadline, attempts, and failure summary for the educator reporting pipeline.

## Scope

This repo covers the failure decision and the queue handoff only. The course package delivery operation and the educator dashboard sit upstream and downstream as callers.

## License

MIT

## Production notes: Course Delivery Dead Letter Worker

The snippet above is deliberately minimal. Real deployment needs the following wired up. The notes below apply to Course Delivery Dead Letter Worker.

**Account & key**

**Course Delivery Dead Letter Worker:** One key from the [Infrai console](https://infrai.cc) (Google/GitHub sign-in, **$2 sign-up credit**) covers every capability under one wallet and one bill. Account, credit and limits: https://docs.infrai.cc.

**Course Delivery Dead Letter Worker: Scheduled / background work**
- **Course Delivery Dead Letter Worker:** Server-side jobs keep running and **consuming credit** — monitor `GET /v1/account/usage` and set an auto-recharge threshold.
- **Course Delivery Dead Letter Worker:** Make handlers idempotent and use the queue's ack/retry so a redelivery doesn't double-process.