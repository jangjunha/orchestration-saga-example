# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is an orchestration saga transaction example implemented in Rust with three microservices: order-service, payment-service, and inventory-service. The project demonstrates the saga pattern for distributed transaction management with compensation logic.

## Architecture

### Microservices
- **order-service**: Orchestrates sagas, manages orders, implements transactional outbox pattern
- **payment-service**: Handles payment processing with compensation (refunds)
- **inventory-service**: Manages inventory reservations with compensation (cancellation)
- **shared**: Common types and saga coordination logic

### Key Components
- **Saga Transaction**: Coordinates multi-step distributed transactions
- **Command-Reply Pattern**: Asynchronous communication between services
- **Idempotent Commands**: Each command includes idempotency key for safe retries
- **Compensation Logic**: Rollback mechanisms for failed transactions
- **Transactional Outbox**: Ensures atomicity when producing messages

### Data Flow
1. Order service creates saga transaction
2. Sends commands to payment and inventory services
3. Services process commands and send replies
4. On failure, compensation commands are sent in reverse order
5. Outbox processor publishes events to Kafka

## Development Commands

### Build and Test
- `cargo build` - Build all workspace members
- `cargo test` - Run all tests
- `cargo check` - Check code without building
- `cargo clippy` - Run linter
- `cargo fmt` - Format code

### Database Operations
- `diesel migration run --database-url postgres://postgres:password@localhost/orders` - Run order service migrations
- `diesel migration run --database-url postgres://postgres:password@localhost/payments` - Run payment service migrations  
- `diesel migration run --database-url postgres://postgres:password@localhost/inventory` - Run inventory service migrations

### Running Services
- `cargo run --bin order-service` - Run order service
- `cargo run --bin payment-service` - Run payment service
- `cargo run --bin inventory-service` - Run inventory service

### Docker Commands
- `docker compose up -d postgres kafka` - Start infrastructure only
- `docker compose up kafka-setup` - Create Kafka topics (runs automatically)
- `docker compose up` - Start all services (includes automatic topic creation)
- `docker compose down` - Stop all services

### Kafka Topics
The following topics are automatically created when starting the services:

**Command Topics:**
- `order-commands` - Commands for order service
- `payment-service-commands` - Commands for payment service  
- `inventory-service-commands` - Commands for inventory service

**Reply Topics:**
- `order-replies` - Saga coordination replies

**Event Topics:**
- `order-events` - Order domain events
- `payment-events` - Payment domain events
- `inventory-events` - Inventory domain events
- `domain-events` - General domain events

## Service Configuration

Each service accepts command-line arguments:
- `--database-url`: PostgreSQL connection string
- `--kafka-brokers`: Kafka broker addresses
- `--command-topic`: Topic for receiving commands
- `--reply-topic`: Topic for sending replies

## Key Features

### Idempotency
Commands include idempotency keys to prevent duplicate processing during retries.

### Compensation
Each saga step can have compensation logic:
- Payment compensation: Refund processed payments
- Inventory compensation: Cancel reservations and restore availability

### Transactional Outbox
Order service uses outbox pattern to ensure atomic message publishing with database transactions.