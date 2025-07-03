diesel::table! {
    orders (id) {
        id -> Uuid,
        customer_id -> Uuid,
        product_id -> Uuid,
        quantity -> Int4,
        total_amount -> Numeric,
        status -> Varchar,
        created_at -> Nullable<Timestamptz>,
        updated_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    outbox_events (id) {
        id -> Uuid,
        aggregate_id -> Uuid,
        event_type -> Varchar,
        event_data -> Jsonb,
        processed -> Nullable<Bool>,
        created_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    processed_commands (idempotency_key) {
        idempotency_key -> Varchar,
        command_id -> Uuid,
        result -> Nullable<Jsonb>,
        processed_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    saga_transactions (id) {
        id -> Uuid,
        steps -> Jsonb,
        current_step -> Int4,
        status -> Varchar,
        context -> Jsonb,
        created_at -> Nullable<Timestamptz>,
        updated_at -> Nullable<Timestamptz>,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    orders,
    outbox_events,
    processed_commands,
    saga_transactions,
);