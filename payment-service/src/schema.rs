diesel::table! {
    payments (id) {
        id -> Uuid,
        order_id -> Uuid,
        amount -> Numeric,
        payment_method -> Varchar,
        status -> Varchar,
        processed_at -> Nullable<Timestamptz>,
        created_at -> Nullable<Timestamptz>,
        updated_at -> Nullable<Timestamptz>,
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

diesel::allow_tables_to_appear_in_same_query!(
    payments,
    processed_commands,
);