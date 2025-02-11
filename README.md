# Cyclonetix

## **Overview**
Cyclonetix is a lightweight, Rust-based workflow orchestrator designed to be **fast, easy to use, and highly scalable**. It is built for **both cloud-native and on-premises deployments**, supporting **outcome-based scheduling, self-assembling DAGs, and manual DAG execution**.

Unlike traditional orchestrators, Cyclonetix aims to be:
- **Effortlessly simple to get started with**
- **Exceptionally fast and lightweight**
- **Flexible enough for both beginners and enterprise-scale workloads**
- **Clever by design, reducing user cognitive load**
- **Capable of advanced scheduling mechanisms** (outcome-based scheduling + explicit DAG execution)

---

## **Architecture**
### **Core Components**
Cyclonetix consists of the following key components:

| Component | Description |
|-----------|------------|
| **Orchestrator** | Builds execution graphs, evaluates task readiness, and schedules work. |
| **Worker** | Picks up tasks from queues and executes them. |
| **Execution Graph** | Self-assembled DAG built from tasks, outcomes, and dependencies. |
| **State Manager (Redis, PostgreSQL, In-Memory Dev Mode)** | Stores task states, execution metadata, and scheduled outcomes. Supports different backends for different environments. |
| **UI (Axum SSR + Tabler + Cytoscape.js)** | Provides real-time execution tracking and DAG visualization. |
| **Authentication (OAuth, Basic Auth, API Keys)** | Ensures secure access to Cyclonetix resources. |


### **Execution Flow**
1. **Users schedule an "Outcome" (final goal/task)**
2. **Orchestrator determines required tasks to complete the Outcome (self-assembling DAG)**
3. **Tasks are assigned to queues, picked up by workers, executed, and tracked in Redis or another backend**
4. **Completed tasks trigger evaluations for the next executable tasks via events**
5. **Execution continues until the full DAG is completed**


### **Scheduling Model**
Cyclonetix supports two scheduling models:
- **Outcome-Based Scheduling** (Self-assembling DAGs)
  - Users schedule an **Outcome** (e.g., `deploy-model`), and Cyclonetix dynamically determines the required execution steps.
- **Manual DAG Execution**
  - Users define DAGs explicitly in YAML and execute them as structured workflows.


### **Contexts & Parameters**
- **Contexts** allow global state to be passed through execution graphs, ensuring variables and configurations are inherited properly.
- **Parameter Sets** allow specific configurations to be associated with tasks, ensuring fine-grained control over execution and reuse of tasks with different "flavourings".  Parameters can be included in dependency declarations.
- **Contexts can be hierarchical**, enabling seamless overrides for scoped execution.  Base contexts loaded from repo and partially overridden by user-defined contexts values at scheduling time.
- **Support for common macros like ${DATE}, ${TIME}, ${RANDOM} for parameter and context values.


### **Evaluation Points**
- **Evaluation tasks** allow dynamic decision-making within a DAG.
- **An evaluation step receives multiple inputs and determines the next workflow step dynamically**.
- **Evaluation tasks are fully user-defined and can run arbitrary logic to decide next execution steps**.
- **Evaluation tasks can be used for conditional branching, error handling, or dynamic DAG construction**.
- **Can be used as an intercept point for manual approval or external system integration**.
- **Enables AI integration for dynamic decision-making based on task outcomes and range of available scheduling options**.
- **Context can be propagated onwards to ensure that any tasks scheduled as a result of the evaluation have the correct context**.


### **Authentication & Security**
- **OAuth2 Support** for enterprise authentication.
- **Basic Auth for simple use cases**.
- **API Keys for programmatic access**.
- **RBAC (Role-Based Access Control) planned for future enterprise usage**.


### **Scaling Model (Kubernetes, Auto-Scaling)**
- **Workers dynamically scale based on queue depth on Kubernetes using Prometheus and HPA**.
- **Orchestrators self-distribute workloads using modulo hashing to allow for orchestrator scale-out on huge deployments**.
- **Affinity Rules allow specific tasks to be bound to specific worker pools (e.g., GPU workers, high-memory workers, compute intensive tasks)**.
- **Labelling of tasks, queues, workers to support cost attribution in cloud environments**.


### **Git Integration for Code Execution**
- **Instead of requiring local script uploads, Cyclonetix allows execution from Git repositories**.
- **Supports versioning of DAGs and tasks using Git branches/tags**.
- **Users can specify Git repo, branch, and execution command for tasks**.


### **Embedded In-Memory Backend for Development**
- **For local development, Cyclonetix supports an in-memory execution mode**.
- **Ensures rapid iteration without requiring Redis or PostgreSQL**.
- **Production environments can configure Redis/PostgreSQL for durability**.


---

## **Roadmap & Next Steps**

### **Immediate Priorities**
**Finalize task recovery logic and orchestrator state handling**
**Prepare documentation (`docs/` folder structure)**
**Ensure Redis-based backend is working fully**
**Develop and test PostgreSQL-based StateManager backend**


### **Short-Term Goals** (Next 2-4 weeks)
- **Build the first version of the UI (Axum SSR + Tabler.js + Cytoscape.js)**
- **Introduce event-driven execution triggers (Kafka, Webhooks, API calls)**
- **Expose REST API for external system integration**
- **Refine Kubernetes auto-scaling logic**
- **Enhance Git-based execution for versioned workflows**


### **Long-Term Vision**
- **Cloud Deployment Automation (Terraform/Pulumi integration)**
- **Jupyter Notebook Execution as DAGs** (Convert annotated notebooks into production-ready workflows)
- **WASM Execution Support** (Ultra-lightweight task execution for simple workloads)
- **Live Execution Tracking UI with WebSockets** (Real-time updates on DAG execution state)
- **Multi-tenancy support for organizations running shared workloads**




