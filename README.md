[![CI Pipeline](https://github.com/neural-chilli/Cyclonetix/actions/workflows/build.yml/badge.svg)](https://github.com/neural-chilli/Cyclonetix/actions/workflows/build.yml)
[![Quarto Docs](https://img.shields.io/badge/docs-online-blue.svg)](https://neural-chilli.github.io/Cyclonetix/)

# Cyclonetix

A lightweight, Rust-based workflow orchestrator designed for speed, simplicity, and scalability.

## Overview

Cyclonetix is a workflow orchestration system that makes complex task orchestration simple. It provides:

- **Self-assembling DAGs**: Schedule outcomes and let Cyclonetix determine the execution path
- **Task Dependency Management**: Define dependencies between tasks to ensure proper execution order
- **Distributed Execution**: Scale from local development to large clusters
- **Flexible Backend Storage**: Use Redis, PostgreSQL, or in-memory options for state management
- **Modern Web UI**: Monitor and manage workflows through an intuitive web interface

Whether you're running data pipelines, ML training jobs, or software deployment processes, Cyclonetix helps orchestrate your workflows with minimal overhead and maximum flexibility.

## Key Features

- **Outcome-Based Scheduling**: Define what you want, not how to get it
- **Self-Assembling DAGs**: Automatic resolution of task dependencies
- **Simple API and CLI**: Easy integration with existing systems
- **Fast Execution**: Lightweight core with minimal overhead
- **Flexible Deployment**: Run locally or scale to large clusters
- **Evaluation Points**: Dynamic decision-making within workflows
- **Context & Parameter Management**: Control workflow behavior with environment variables and parameters
- **Git Integration**: Execute code directly from Git repositories
- **Web UI Dashboard**: Monitor and manage workflows visually

## Getting Started

### Quick Installation

```bash
# Clone the repository
git clone https://github.com/neural-chilli/Cyclonetix.git
cd Cyclonetix

# Build the project
cargo build --release

# Run with in-memory backend (development mode)
./target/release/cyclonetix
```

### Define Your First Workflow

Create task definitions:

```yaml
# data/tasks/prepare_data.yaml
id: "prepare_data"
name: "Prepare Data"
command: "echo 'Preparing data...'; sleep 2; echo 'Data ready'"
dependencies: []
```

```yaml
# data/tasks/process_data.yaml
id: "process_data"
name: "Process Data"
command: "echo 'Processing data...'; sleep 3; echo 'Processing complete'"
dependencies: ["prepare_data"]
```

### Schedule a Workflow

Schedule by outcome (let Cyclonetix determine the execution path):

```bash
./target/release/cyclonetix schedule-task process_data
```

Or define and schedule a complete DAG:

```yaml
# data/dags/data_pipeline.yaml
id: "data_pipeline"
name: "Data Pipeline"
tasks:
  - id: "prepare_data"
  - id: "process_data"
```

```bash
./target/release/cyclonetix schedule-dag data_pipeline
```

### Monitor Execution

Open your browser to `http://localhost:3000` to see the Cyclonetix UI, where you can:

- View workflow execution progress
- Inspect task details and logs
- Visualize DAG structure
- Schedule new workflows

## Architecture

Cyclonetix consists of several core components:

- **Orchestrator**: Builds execution graphs, evaluates task readiness, and schedules work
- **Agent**: Picks up tasks from queues and executes them
- **State Manager**: Stores task states, execution metadata, and scheduled outcomes
- **UI**: Provides real-time execution tracking and DAG visualization

These components can run together in a single process or distributed across multiple machines for scalability.

## Production Deployment

For production deployments, use Redis or PostgreSQL as a backend:

```yaml
# config.yaml
backend: "redis"
backend_url: "redis://localhost:6379"
queues:
  - "default"
  - "high_memory"
  - "gpu_tasks"
```

Run Cyclonetix with your configuration:

```bash
./target/release/cyclonetix --config config.yaml
```

For more advanced deployments, check the [documentation](https://neural-chilli.github.io/Cyclonetix/deployment/installation.html).

## Advanced Features

### Contexts and Parameters

Define environment contexts:

```yaml
# data/contexts/production.yaml
id: "production"
variables:
  ENV: "production"
  LOG_LEVEL: "info"
  DATA_PATH: "/data/production"
```

Schedule with context:

```bash
./target/release/cyclonetix schedule-task process_data --context production
```

### Evaluation Points

Create dynamic workflows with evaluation points:

```yaml
id: "evaluate_data"
name: "Evaluate Data Quality"
command: "python evaluate.py --input ${INPUT_PATH} --output $CYCLO_EVAL_RESULT"
dependencies: ["prepare_data"]
evaluation_point: true
```

The evaluation script can dynamically determine next steps based on data quality.

### Git Integration

Execute code directly from Git repositories:

```yaml
command: "git-exec https://github.com/example/repo.git scripts/process.py --arg ${VALUE}"
```

## Documentation

Comprehensive documentation is available at [https://neural-chilli.github.io/Cyclonetix/](https://neural-chilli.github.io/Cyclonetix/), including:

- [Getting Started Guide](https://neural-chilli.github.io/Cyclonetix/getting-started.html)
- [Core Concepts](https://neural-chilli.github.io/Cyclonetix/core-concepts/architecture.html)
- [User Guide](https://neural-chilli.github.io/Cyclonetix/user-guide/task-definition.html)
- [Deployment Guide](https://neural-chilli.github.io/Cyclonetix/deployment/installation.html)
- [API Reference](https://neural-chilli.github.io/Cyclonetix/reference/api.html)
- [Developer Guide](https://neural-chilli.github.io/Cyclonetix/developer-guide.html)

## Development

For UI development, enable template hot-reloading:

```bash
DEV_MODE=true cargo run
```

This mode:
- Disables Tera template caching
- Reloads templates on each request
- Enables faster UI development workflow

## Roadmap

Our development roadmap includes:

- **Short-term**: UI improvements, REST API, Kubernetes auto-scaling
- **Medium-term**: Workflow versioning, conditional branching, RBAC
- **Long-term**: Multi-tenancy, serverless execution, AI-driven optimization

See the [full roadmap](https://neural-chilli.github.io/Cyclonetix/roadmap.html) for details.

## Contributing

Contributions are welcome! See the [Developer Guide](https://neural-chilli.github.io/Cyclonetix/developer-guide.html) for information on getting started with Cyclonetix development.

## License

Cyclonetix is licensed under the [MIT License](LICENSE).