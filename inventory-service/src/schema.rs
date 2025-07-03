diesel::table! {
    inventory (id) {
        id -> Uuid,
        product_id -> Uuid,
        available_quantity -> Int4,
        reserved_quantity -> Int4,
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

diesel::table! {
    reservations (id) {
        id -> Uuid,
        product_id -> Uuid,
        order_id -> Uuid,
        quantity -> Int4,
        status -> Varchar,
        created_at -> Nullable<Timestamptz>,
        updated_at -> Nullable<Timestamptz>,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    inventory,
    processed_commands,
    reservations,
);