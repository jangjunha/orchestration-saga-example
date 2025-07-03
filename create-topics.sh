#!/bin/bash

echo "Waiting for Kafka to be ready..."
until kafka-topics --bootstrap-server kafka:29092 --list; do
  echo "Kafka is unavailable - sleeping"
  sleep 2
done

echo "Creating Kafka topics..."

# Command topics for each service
kafka-topics --bootstrap-server kafka:29092 --create --if-not-exists \
  --topic order-service-commands --partitions 3 --replication-factor 1

kafka-topics --bootstrap-server kafka:29092 --create --if-not-exists \
  --topic payment-service-commands --partitions 3 --replication-factor 1

kafka-topics --bootstrap-server kafka:29092 --create --if-not-exists \
  --topic inventory-service-commands --partitions 3 --replication-factor 1

# Reply topic for saga coordination
kafka-topics --bootstrap-server kafka:29092 --create --if-not-exists \
  --topic order-replies --partitions 3 --replication-factor 1

# Event topics for domain events
kafka-topics --bootstrap-server kafka:29092 --create --if-not-exists \
  --topic order-events --partitions 3 --replication-factor 1

kafka-topics --bootstrap-server kafka:29092 --create --if-not-exists \
  --topic payment-events --partitions 3 --replication-factor 1

kafka-topics --bootstrap-server kafka:29092 --create --if-not-exists \
  --topic inventory-events --partitions 3 --replication-factor 1

kafka-topics --bootstrap-server kafka:29092 --create --if-not-exists \
  --topic domain-events --partitions 3 --replication-factor 1

echo "Topics created successfully:"
kafka-topics --bootstrap-server kafka:29092 --list

echo "Topic creation completed!"