# Cyclonetix
Blazing fast task orchestration framework


## **1. Overview**
This document outlines the detailed design of a **high-performance, AI-friendly orchestration framework** built in Rust. The framework provides **dynamic task execution, self-assembling graphs, flexible evaluation points, and cloud-native scaling** while maintaining simplicity and efficiency.

### **Key Features**
- **Single-binary dev mode** with **embedded Redis** for local development.
- **Dynamic graph execution** built from task dependencies at scheduling time.
- **Evaluation tasks** that allow **runtime decision-making**, including LLM integration.
- **Kubernetes-native autoscaling** with **Prometheus-based scaling signals.**
- **Task execution with contextual variable propagation.**
- **Cytoscape.js-powered UI** for visualizing execution graphs.

---

## **2. Redis Strategy**
### **2.1. Embedded Redis for Development**
- The framework will **embed a standard Redis binary** for local development to ensure consistency with cloud deployments.
- Developers can spin up the entire system with **one binary**, without needing to install Redis manually.
- If users want persistence across restarts, they can run their **own Redis instance**.

### **2.2. External Redis for Production**
- On-premise users will be able to **point the framework to an external Redis instance**.
- Cloud deployments will use **Redis PaaS (e.g., AWS ElastiCache, GCP Memorystore).**

---

## **3. Execution Model**
### **3.1. Single Binary Dev Mode**
- The entire framework runs as **a single binary** in development.
- In production, it can be **split into separate services** (UI, orchestrator, worker).

### **3.2. Multiple Orchestrator Pattern**
- **Orchestrators distribute execution graphs using `mod n` of a hash.**
- **No leader election or complex clustering needed**—each orchestrator knows its assigned tasks.
- If an orchestrator crashes, a new instance will **rebuild execution graphs from Redis.**

### **3.3. Stateless UI with Cytoscape.js**
- The UI is stateless and purely **reads execution data** from Redis.
- **Graphs will be rendered using Cytoscape.js**, supporting **real-time updates**.

### **3.4. Workers Polling Redis for Work**
- Workers **poll Redis queues** for available tasks.
- Each worker **processes only tasks for queues they are assigned to**.

### **3.5. Dynamic Queue Creation**
- **Workers service specific queues**, which are **dynamically created** as needed.
- Queues can be **explicitly assigned** to specific worker types.

### **3.6. Cost Attribution via Labels**
- Workers and queues can be **tagged with labels** (e.g., `department: AI-Research`).
- **Cloud billing tools can track usage per department.**

---

## **4. Execution Context and Variables**
### **4.1. Context Propagation**
- **Each scheduled graph has a `context` map.**
- Context **inherits values from the parent workflow** when one schedules another.
- Context is **exposed to tasks as environment variables**.

### **4.2. Task + Context = Unique Execution**
- Task executions are uniquely identified by **task name + context**.
- Tasks can **add new values** to the execution context.

### **4.3. Handling Special Variables**
- Variables like `DATE` are **resolved at scheduling time**, ensuring consistency.
- **YAML supports `${VARIABLE}` macros**, expanded at scheduling.

---

## **5. Graph Assembly and Scheduling**
### **5.1. Self-Assembling Execution Graphs**
- **Graphs are built dynamically from task dependencies** at scheduling time.
- Orchestrators **cache graphs in memory** but can rebuild them from Redis.

### **5.2. Global Tasks as an Option**
- Tasks can be marked **as global**, meaning **all graphs update** when they complete.
- Default behavior is **per-graph execution.**

### **5.3. Worker/Queue/Task Affinity**
- Some tasks require **GPU, high-memory, or CPU-optimized workers**.
- Worker affinity allows tasks to **run on appropriate hardware.**

---

## **6. Evaluation-Based Execution**
### **6.1. Evaluation Tasks**
- Instead of **predefined conditionals**, tasks can specify **evaluation points**.
- Evaluator tasks **return JSON via stdout or a file**.
- JSON specifies **which workflows (if any) should run next**.
- Evaluators **can be simple Python logic or AI-driven (e.g., LangChain + LLMs).**

### **6.2. Task Descriptions for AI Evaluators**
- Tasks must include **rich descriptions** so LLM-based evaluators can understand them.

---

## **7. Autoscaling Strategy**
### **7.1. Kubernetes Scaling via Prometheus**
- **Orchestrator calculates worker needs** instead of Kubernetes linking directly to Redis.
- **Prometheus metric `orchestrator_desired_workers`** tells Kubernetes how many workers are needed.

### **7.2. Worker Pool Scaling Algorithm**
- **Each worker pool has:**
    - `min`: Minimum worker count
    - `max`: Maximum worker count
    - `factor`: Scaling factor based on queue depth
- Example: If queue depth is 18 and **factor is 6**, Kubernetes **scales to 3 workers (min 0, max 4).**

### **7.3. Fine-Grained Worker Scaling**
- Scaling requests specify **exact worker types and counts**, e.g.:
  ```plaintext
  orchestrator_desired_workers{worker_type="gpu"} 7
  orchestrator_desired_workers{worker_type="cpu"} 3
  ```
- Kubernetes scales each worker type **independently**.

---

## **8. Configuration and APIs**
### **8.1. YAML-Based Task Definitions**
- Users can **bootstrap instances** from YAML task definitions.
- Supports **version control** and **cloud storage for config.**

### **8.2. UI for Task Definitions**
- Users can **define tasks in the UI**, which are stored as YAML.

### **8.3. REST API**
- Schedule/de-schedule tasks
- Query task statuses, queues, executors
- View scheduled graphs in real-time

### **8.4. Authentication**
- **OAuth, Basic Auth, or other configurable methods.**

---

## **9. Logging and Observability**
### **9.1. Structured Logging with Tracing**
- Uses **tracing-based structured logs** instead of raw log messages.

### **9.2. UI Metrics and Real-Time Updates**
- UI displays **work in queues, graph execution, executor statuses, and logs.**

---

## **Next Steps**
1. **Finalize YAML format and variable handling.**
2. **Prototype Redis-based execution graph caching.**
3. **Implement evaluation tasks with JSON-based decisioning.**
4. **Test autoscaling logic in Kubernetes.**

