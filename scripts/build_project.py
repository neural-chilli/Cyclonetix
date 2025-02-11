import os

# Define the project structure
project_root = "cyclonetix"
structure = {
    "src": ["main.rs"],
    "src/orchestrator": [
        "mod.rs",
        "engine.rs",
        "scheduler.rs",
        "state_manager.rs",
        "redis_state_manager",
        "memory_backend.rs",
        "executor_tracker.rs",
    ],
    "src/workers": ["mod.rs", "runner.rs", "queues.rs"],
    "src/models": ["mod.rs", "task.rs", "context.rs", "graph.rs", "executor.rs"],
    "src/api": ["mod.rs", "routes.rs", "handlers.rs"],
    "src/ui": ["mod.rs", "server.rs", "graph_api.rs"],
    "src/utils": ["mod.rs", "logging.rs", "config.rs", "tracing.rs"],
    "tests": [],
    "examples": [],
    "migrations": [],
    "scripts": [],
}

# Define default content for mod.rs files
mod_contents = {
    "src/orchestrator/mod.rs": "pub mod engine;\npub mod scheduler;\npub mod state_manager;\npub mod redis_backend;\npub mod memory_backend;\npub mod executor_tracker;\n",
    "src/workers/mod.rs": "pub mod runner;\npub mod queues;\n",
    "src/models/mod.rs": "pub mod task;\npub mod context;\npub mod graph;\npub mod executor;\n",
    "src/api/mod.rs": "pub mod routes;\npub mod handlers;\n",
    "src/ui/mod.rs": "pub mod server;\npub mod graph_api;\n",
    "src/utils/mod.rs": "pub mod logging;\npub mod config;\npub mod tracing;\n",
}

# Create directories and files
for directory, files in structure.items():
    os.makedirs(os.path.join(project_root, directory), exist_ok=True)
    for file in files:
        file_path = os.path.join(project_root, directory, file)
        with open(file_path, "w") as f:
            if file.endswith("mod.rs") and file_path in mod_contents:
                f.write(mod_contents[file_path])

print(f"Project structure for '{project_root}' has been created.")
