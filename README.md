# Orchestration Saga Transaction Example

A comprehensive implementation of the **Orchestration Saga Pattern** using Rust, demonstrating distributed transaction management across three microservices with command-reply communication, compensation logic, and reliable message delivery.

## 🏗️ Architecture Overview

This project implements an **Order Processing System** using the orchestration saga pattern to coordinate transactions across multiple microservices:

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Order Service │    │ Payment Service │    │Inventory Service│
│                 │    │                 │    │                 │
│ • Web API       │    │ • Process       │    │ • Reserve       │
│ • Saga Manager  │◄──►│   Payment       │    │   Inventory     │
│ • Order CRUD    │    │ • Refund        │    │ • Release       │
│                 │    │   Payment       │    │   Inventory     │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 │
                    ┌─────────────────┐
                    │     Kafka       │
                    │  Message Broker │
                    └─────────────────┘
                                 │
                    ┌─────────────────┐
                    │   PostgreSQL    │
                    │    Database     │
                    └─────────────────┘
```

## 🎯 Features

### Core Saga Implementation
- **✅ Orchestration Pattern**: Centralized saga coordinator managing distributed transactions
- **✅ Command-Reply Communication**: Asynchronous messaging between services
- **✅ Compensation Logic**: Automatic rollback of completed steps on failure
- **✅ Idempotency**: Safe command retry with deduplication
- **✅ Transactional Outbox**: Reliable message delivery pattern

### Business Logic
- **✅ Order Management**: Complete order lifecycle (created → approved/cancelled)
- **✅ Payment Processing**: Payment processing with refund capability
- **✅ Inventory Management**: Product reservation with validation
- **✅ Cross-Service Consistency**: ACID properties across distributed services

### Technical Features
- **✅ Microservices Architecture**: Three independent services with separate databases
- **✅ Event-Driven Communication**: Kafka-based messaging
- **✅ Database Migrations**: Automatic schema management with Diesel
- **✅ Docker Containerization**: Complete development environment
- **✅ Health Checks**: Service readiness and dependency management

## 🚀 Quick Start

### Prerequisites
- Docker and Docker Compose
- Rust 1.88+ (for development)

### Running the System

1. **Clone the repository**:
   ```bash
   git clone <repository-url>
   cd orchestration-saga-example
   ```

2. **Start all services**:
   ```bash
   docker compose up -d --build
   ```

3. **Verify services are running**:
   ```bash
   docker compose ps
   ```

4. **Test the health endpoint**:
   ```bash
   curl http://localhost:3001/health
   # Expected: OK
   ```

### Testing the Saga

#### ✅ **Successful Transaction** (All steps complete):
```bash
curl -X POST http://localhost:3001/orders \
  -H "Content-Type: application/json" \
  -d '{
    "customer_id": "550e8400-e29b-41d4-a716-446655440000",
    "product_id": "11111111-1111-1111-1111-111111111111",
    "quantity": 2,
    "total_amount": 99.99
  }'
```
**Expected Result**: Order status "approved", all steps completed successfully.

#### ❌ **Payment Failure** (Random 20% failure rate):
```bash
curl -X POST http://localhost:3001/orders \
  -H "Content-Type: application/json" \
  -d '{
    "customer_id": "550e8400-e29b-41d4-a716-446655440001",
    "product_id": "11111111-1111-1111-1111-111111111111",
    "quantity": 1,
    "total_amount": 149.99
  }'
```
**Expected Result**: If payment fails, order status "cancelled" with compensation.

#### ❌ **Inventory Failure** (Non-existent product):
```bash
curl -X POST http://localhost:3001/orders \
  -H "Content-Type: application/json" \
  -d '{
    "customer_id": "550e8400-e29b-41d4-a716-446655440002",
    "product_id": "99999999-9999-9999-9999-999999999999",
    "quantity": 1,
    "total_amount": 199.99
  }'
```
**Expected Result**: Payment refunded, order status "cancelled" with full compensation.

## 📋 Saga Flow

### Forward Flow (Success Path)
1. **CreateOrder**: Order created with status "created"
2. **ProcessPayment**: Payment processed and recorded
3. **ReserveInventory**: Inventory reserved for the product
4. **ApproveOrder**: Order status changed to "approved"

### Compensation Flow (Failure Path)
When any step fails, compensation occurs in reverse order:
1. **CompensateInventory**: Release reserved inventory (if applicable)
2. **CompensatePayment**: Refund the payment (if applicable)
3. **CancelOrder**: Change order status to "cancelled"

## 🗄️ Database Schema

### Order Service Database (`orders`)
```sql
-- Orders table
CREATE TABLE orders (
    id UUID PRIMARY KEY,
    customer_id UUID NOT NULL,
    product_id UUID NOT NULL,
    quantity INTEGER NOT NULL,
    total_amount DECIMAL NOT NULL,
    status VARCHAR NOT NULL, -- 'created', 'approved', 'cancelled'
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW()
);

-- Saga transactions table
CREATE TABLE saga_transactions (
    id UUID PRIMARY KEY,
    steps JSONB NOT NULL,
    current_step INTEGER NOT NULL,
    status VARCHAR NOT NULL, -- 'Started', 'InProgress', 'Completed', 'Compensating', 'Compensated'
    context JSONB NOT NULL,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW()
);

-- Command processing table (idempotency)
CREATE TABLE processed_commands (
    idempotency_key VARCHAR PRIMARY KEY,
    command_id UUID NOT NULL,
    result JSONB,
    processed_at TIMESTAMP DEFAULT NOW()
);

-- Transactional outbox table
CREATE TABLE outbox_events (
    id UUID PRIMARY KEY,
    aggregate_id UUID NOT NULL,
    event_type VARCHAR NOT NULL,
    event_data JSONB NOT NULL,
    processed BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP DEFAULT NOW()
);
```

### Payment Service Database (`payments`)
```sql
CREATE TABLE payments (
    id UUID PRIMARY KEY,
    order_id UUID NOT NULL,
    amount DECIMAL NOT NULL,
    payment_method VARCHAR NOT NULL,
    status VARCHAR NOT NULL, -- 'processed', 'refunded'
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW()
);
```

### Inventory Service Database (`inventory`)
```sql
CREATE TABLE inventory (
    id UUID PRIMARY KEY,
    product_id UUID NOT NULL,
    quantity INTEGER NOT NULL,
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW()
);

CREATE TABLE reservations (
    id UUID PRIMARY KEY,
    product_id UUID NOT NULL,
    order_id UUID NOT NULL,
    quantity INTEGER NOT NULL,
    status VARCHAR NOT NULL, -- 'reserved', 'released'
    created_at TIMESTAMP DEFAULT NOW(),
    updated_at TIMESTAMP DEFAULT NOW()
);
```

## 🔄 Message Flow

### Kafka Topics
- **`order-service-commands`**: Commands for order service
- **`payment-service-commands`**: Commands for payment service  
- **`inventory-service-commands`**: Commands for inventory service
- **`order-replies`**: Command replies for saga coordination
- **`*-events`**: Domain events for each service

### Command Types
```rust
pub enum CommandType {
    CreateOrder,        // Create a new order
    ProcessPayment,     // Process payment for order
    ReserveInventory,   // Reserve product inventory
    ApproveOrder,       // Mark order as approved
    CompensatePayment,  // Refund payment (compensation)
    CompensateInventory,// Release inventory (compensation)
    CancelOrder,        // Mark order as cancelled (compensation)
}
```

## 🐳 Services

### Order Service (Port 3001)
- **REST API**: Accepts HTTP requests to create orders
- **Saga Coordinator**: Manages distributed transaction flow
- **Reply Handler**: Processes command replies and advances saga steps
- **Database**: Stores orders, saga state, and processed commands

### Payment Service (Port 3002)
- **Payment Processing**: Simulates payment with 80% success rate
- **Refund Processing**: Handles payment compensation
- **Database**: Stores payment records and transaction history

### Inventory Service (Port 3003)
- **Inventory Management**: Reserves and releases product inventory
- **Product Validation**: Special product ID `11111111-1111-1111-1111-111111111111` always succeeds
- **Database**: Stores inventory levels and reservations

## 🛠️ Development

### Building Locally
```bash
# Build all services
cargo build --release

# Run tests
cargo test

# Check code formatting
cargo fmt --check

# Run linting
cargo clippy
```

### Environment Variables
```bash
# Database connections
DATABASE_URL=postgres://postgres@postgres/orders
KAFKA_BROKERS=kafka:29092

# Service ports
PORT=3001  # Order service
PORT=3002  # Payment service  
PORT=3003  # Inventory service
```

### Monitoring

#### View Logs
```bash
# All services
docker compose logs -f

# Specific service
docker compose logs -f order-service
docker compose logs -f payment-service
docker compose logs -f inventory-service
```

#### Database Access
```bash
# Connect to orders database
docker exec -it orchestration-saga-example-postgres-1 psql -U postgres -d orders

# Connect to payments database
docker exec -it orchestration-saga-example-postgres-1 psql -U postgres -d payments

# Connect to inventory database
docker exec -it orchestration-saga-example-postgres-1 psql -U postgres -d inventory
```

#### Kafka Topics
```bash
# List topics
docker exec kafka kafka-topics --bootstrap-server localhost:9092 --list

# View messages in a topic
docker exec kafka kafka-console-consumer --bootstrap-server localhost:9092 --topic order-replies --from-beginning
```

## 🔍 Key Implementation Details

### Idempotency
Each command includes an idempotency key to ensure safe retries:
```rust
pub struct Command {
    pub id: Uuid,
    pub saga_id: Uuid,
    pub command_type: CommandType,
    pub payload: serde_json::Value,
    pub idempotency_key: String, // Prevents duplicate processing
    pub created_at: DateTime<Utc>,
}
```

### Transactional Outbox
Database changes and message publishing are atomic:
```rust
// Within database transaction
conn.transaction(|conn| {
    // 1. Update business data
    diesel::insert_into(orders::table).values(&order).execute(conn)?;
    
    // 2. Store outbox event
    diesel::insert_into(outbox_events::table).values(&event).execute(conn)?;
    
    Ok(())
}).await?;

// 3. Separate process publishes outbox events to Kafka
```

### Compensation Logic
Failed sagas trigger compensation in reverse order:
```rust
async fn start_compensation(&self, saga: &mut SagaTransaction) -> Result<()> {
    let compensation_steps = saga.get_compensation_steps(); // Reverse order
    
    for step in compensation_steps {
        if let Some(compensation_type) = &step.compensation_type {
            let command = create_compensation_command(compensation_type, saga)?;
            self.send_command(&command, &step.service_name).await?;
        }
    }
}
```

## 🎓 Learning Outcomes

This project demonstrates:

1. **Distributed Transaction Patterns**: Saga pattern vs 2PC
2. **Event-Driven Architecture**: Async messaging and event sourcing
3. **Microservices Coordination**: Service communication and data consistency
4. **Error Handling**: Graceful failure handling and compensation
5. **Database Design**: Multi-tenant database patterns
6. **Container Orchestration**: Docker Compose and service dependencies
7. **Message Broker Integration**: Kafka producers, consumers, and topics
8. **API Design**: RESTful APIs and async command processing

## 🚨 Production Considerations

For production deployment, consider:

- **Service Discovery**: Replace hardcoded hostnames
- **Secret Management**: Externalize database credentials
- **Monitoring**: Add metrics, tracing, and alerting
- **Load Balancing**: Scale services horizontally
- **Database HA**: PostgreSQL clustering and replication
- **Kafka Cluster**: Multi-broker setup with replication
- **Security**: TLS, authentication, and authorization
- **Circuit Breakers**: Fault tolerance patterns
- **Saga Persistence**: Durable saga state storage
- **Dead Letter Queues**: Handle poison messages

## 📚 References

- [Saga Pattern](https://microservices.io/patterns/data/saga.html)
- [Transactional Outbox](https://microservices.io/patterns/data/transactional-outbox.html)
- [Event Sourcing](https://martinfowler.com/eaaDev/EventSourcing.html)
- [Diesel ORM Documentation](https://diesel.rs/)
- [Apache Kafka Documentation](https://kafka.apache.org/documentation/)

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.