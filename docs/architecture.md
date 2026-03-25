# Architecture

This directory contains the technical design for Snif.

## Recommended Reading Order

1. [Product and Delivery](./product-and-delivery.md)
2. [Detailed Architecture](./05_dev_agent_architecture.adoc)

## What the Architecture Covers

The detailed architecture document describes:

- the system boundaries and external dependencies
- the chosen architectural style
- the core modules and their responsibilities
- the runtime flow for review and evaluation
- context retrieval and budgeting strategy
- output filtering and annotation lifecycle handling
- deployment, observability, and operational constraints

## Design Summary

Snif is designed as a layered modular monolith with ports and adapters at the system boundary. The application core owns workflow orchestration, deterministic context assembly, prompt construction, structured output handling, and evaluation. External systems such as repository platforms and OpenCode are accessed through thin integration layers.

The architecture is intentionally conservative in Phase 1:

- one primary review workflow
- one review agent per change execution
- deterministic, diff-first retrieval
- no vector database or semantic retrieval dependency
- benchmark-driven iteration
