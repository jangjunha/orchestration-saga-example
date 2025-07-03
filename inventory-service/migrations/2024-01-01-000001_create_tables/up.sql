CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE inventory (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    product_id UUID NOT NULL UNIQUE,
    available_quantity INTEGER NOT NULL DEFAULT 0,
    reserved_quantity INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE TABLE reservations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    product_id UUID NOT NULL,
    order_id UUID NOT NULL,
    quantity INTEGER NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'reserved',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE TABLE processed_commands (
    idempotency_key VARCHAR(255) PRIMARY KEY,
    command_id UUID NOT NULL,
    result JSONB,
    processed_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

INSERT INTO inventory (product_id, available_quantity) VALUES
    ('11111111-1111-1111-1111-111111111111', 100),
    ('22222222-2222-2222-2222-222222222222', 50),
    ('33333333-3333-3333-3333-333333333333', 25);

CREATE INDEX idx_inventory_product_id ON inventory(product_id);
CREATE INDEX idx_reservations_order_id ON reservations(order_id);
CREATE INDEX idx_reservations_product_id ON reservations(product_id);