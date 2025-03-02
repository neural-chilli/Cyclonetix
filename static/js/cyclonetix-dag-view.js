
    const dagRunId = new URLSearchParams(window.location.search).get('run_id');
    if (!dagRunId) {
    document.getElementById('dag-container').innerHTML =
        '<div class="alert alert-danger m-3">No DAG run ID provided</div>';
}

    let cy;
    let refreshCountdown = 10;
    let refreshTimer;
    let selectedTask = null;
    let initialLoad = true;
    let allTasksData = [];  // Store all tasks data
    let dagData = null;     // Store DAG data

    const statusColors = {
    'pending': '#6c757d',
    'queued': '#17a2b8',
    'running': '#dc8845',
    'completed': '#28a745',
    'failed': '#dc3545'
};

    document.addEventListener('DOMContentLoaded', function () {
    initCytoscape();
    loadDagData();
    startRefreshTimer();

    document.getElementById('refresh-btn').addEventListener('click', function () {
    loadDagData();
    resetRefreshTimer();
});

    document.getElementById('close-task-info').addEventListener('click', function () {
    hideTaskInfo();
});

    // Add event listeners for task action buttons
    document.getElementById('btn-rerun-task').addEventListener('click', function(event) {
    event.stopPropagation(); // Prevent event from bubbling up
    if (selectedTask) {
    rerunTask(selectedTask.id);
}
});

    document.getElementById('btn-view-logs').addEventListener('click', function(event) {
    event.stopPropagation(); // Prevent event from bubbling up
    if (selectedTask) {
    viewTaskLogs(selectedTask.id);
}
});

    // Prevent clicks inside task info panel from closing it
    document.getElementById('task-info-panel').addEventListener('click', function(event) {
    event.stopPropagation(); // Prevent event from reaching the cy tap handler
});

    // Add keyboard event listener for ESC key
    document.addEventListener('keydown', function(event) {
    if (event.key === 'Escape' && document.getElementById('task-info-panel').style.display === 'block') {
    hideTaskInfo();
}
});
});

    function initCytoscape() {
    cytoscapeDagre = window.cytoscapeDagre;
    cytoscape.use(cytoscapeDagre);

    cy = cytoscape({
    container: document.getElementById('dag-container'),
    zoom: 1,
    minZoom: 0.2,
    maxZoom: 2,
    style: [
{
    selector: 'node',
    style: {
    'background-color': '#666',
    'label': 'data(label)',
    'color': '#fff',
    'font-size': '12px',
    'text-valign': 'center',
    'text-halign': 'center',
    'text-wrap': 'wrap',
    'text-max-width': '80px',
    'width': '80px',
    'height': '50px',
    'shape': 'roundrectangle',
    'border-width': '0',
    'border-color': '#999999',
    'text-margin-y': '0'
}
},
{
    selector: 'edge',
    style: {
    'width': 2,
    'line-color': '#888',
    'target-arrow-color': '#888',
    'target-arrow-shape': 'triangle',
    'curve-style': 'bezier'
}
},
{
    selector: '.status-pending',
    style: { 'background-color': statusColors.pending }
},
{
    selector: '.status-queued',
    style: { 'background-color': statusColors.queued }
},
{
    selector: '.status-running',
    style: { 'background-color': statusColors.running }
},
{
    selector: '.status-completed',
    style: { 'background-color': statusColors.completed }
},
{
    selector: '.status-failed',
    style: { 'background-color': statusColors.failed }
},
{
    selector: '.selected',
    style: {
    'border-width': '3px',
    'border-color': '#f8f9fa'
}
}
    ],
    layout: {
    name: 'dagre',
    rankDir: 'LR',
    padding: 50,
    spacingFactor: 1.25
}
});

    cy.on('tap', 'node', function (event) {
    const node = event.target;
    selectTask(node);
});

    cy.on('tap', function (event) {
    const panel = document.getElementById('task-info-panel');
    // Check if the tap event originated from within the task info panel
    if (panel && panel.contains(event.originalEvent.target)) {
    return; // Do nothing if clicking inside the panel
}
    // Only hide the panel if the click is on the background (i.e. the Cytoscape container itself)
    if (event.target === cy) {
    hideTaskInfo();
}
});
}

    function loadDagData() {
    if (!dagRunId) return;
    fetch(`/api/dag/${dagRunId}`)
    .then(response => response.json())
    .then(data => {
    dagData = data.dag;
    allTasksData = data.tasks || [];
    updateDagInfo(data.dag);
    renderDag(data.tasks, data.graph);
})
    .catch(error => {
    console.error('Error loading DAG data:', error);
    document.getElementById('dag-container').innerHTML =
    `<div class="alert alert-danger m-3">Error loading DAG: ${error.message}</div>`;
});
}

    function updateDagInfo(dag) {
    if (!dag) return;
    document.getElementById('dag-name').textContent = dag.run_id.replace(/\|\|/g, '   -    run ID: ');
}

    function renderDag(tasks, graph) {
    if (!cy) return;

    const currentPan = cy.pan();
    const currentZoom = cy.zoom();

    cy.batch(() => {
    cy.elements().remove();

    if (graph && graph.graph && graph.graph.graph && graph.graph.node_map) {
    const nodesArray = graph.graph.graph.nodes;
    const nodeMap = graph.graph.node_map;
    const reverseNodeMap = {};
    Object.entries(nodeMap).forEach(([taskId, index]) => {
    reverseNodeMap[index] = taskId;
});

    Object.entries(nodeMap).forEach(([taskId, nodeIndex]) => {
    let fullLabel = nodesArray[nodeIndex] || taskId;
    let label = fullLabel.includes('||') ? fullLabel.split('||')[0].trim() : fullLabel;

    // First try to find the task by task_id
    let matchingTask = tasks.find(t => t.task_id === taskId);

    // If not found, try to find by run_id matching the fullLabel
    if (!matchingTask) {
    matchingTask = tasks.find(t => t.run_id === fullLabel);
}

    const statusClass = matchingTask ? `status-${matchingTask.status.toLowerCase()}` : 'status-pending';

    cy.add({
    group: 'nodes',
    data: {
    id: taskId,
    label: label,
    fullLabel: fullLabel,
    taskId: taskId,
    runId: fullLabel,
    status: matchingTask?.status || 'pending'
},
    classes: statusClass
});
});

    const edgesArray = graph.graph.graph.edges;
    edgesArray.forEach(edgeArr => {
    let sourceIndex = edgeArr[0];
    let targetIndex = edgeArr[1];
    let sourceId = reverseNodeMap[sourceIndex];
    let targetId = reverseNodeMap[targetIndex];
    if (sourceId && targetId) {
    cy.add({
    group: 'edges',
    data: { id: `e-${sourceId}-${targetId}`, source: sourceId, target: targetId }
});
}
});
} else if (tasks && tasks.length > 0) {
    tasks.forEach(task => {
    cy.add({
    group: 'nodes',
    data: { id: task.run_id, label: task.name, task: task },
    classes: `status-${task.status.toLowerCase()}`
});
});
    tasks.forEach(task => {
    if (task.dependencies && Array.isArray(task.dependencies)) {
    task.dependencies.forEach(depId => {
    cy.add({
    group: 'edges',
    data: { source: depId, target: task.run_id }
});
});
}
});
} else {
    console.log("No graph or tasks data available to render nodes.");
}
});

    const layoutOptions = {
    name: 'dagre',
    rankDir: 'LR',
    padding: 50,
    spacingFactor: 1.25
};
    cy.layout(layoutOptions).run();

    if (initialLoad) {
    cy.fit();
    initialLoad = false;
} else {
    cy.zoom(currentZoom);
    cy.pan(currentPan);
}
}

    function selectTask(node) {
    cy.nodes().removeClass('selected');
    node.addClass('selected');
    const data = node.data();
    selectedTask = data;

    // Find the full task data
    getTaskInfo(data).then(taskInfo => {
    showTaskInfo(data, taskInfo);
});
}

    async function getTaskInfo(nodeData) {
    const runId = nodeData.runId;

    if (!runId) {
    return null;
}

    try {
    // First try to get detailed info from the API
    const response = await fetch(`/api/task_instance/${runId}`);

    if (response.ok) {
    return await response.json();
} else {
    console.warn(`API returned ${response.status} for task ${runId}`);
}
} catch (error) {
    console.error(`Error fetching task instance data for ${runId}:`, error);
}

    // Fall back to local data if API call fails
    let taskInfo = allTasksData.find(t => t.run_id === runId);

    // If not found and we have a task_id, try that
    if (!taskInfo && nodeData.taskId) {
    taskInfo = allTasksData.find(t => t.task_id === nodeData.taskId);
}

    return taskInfo || null;
}

    function showTaskInfo(nodeData, taskInfo) {
    const panel = document.getElementById('task-info-panel');

    // Set basic info
    document.getElementById('task-info-title').innerHTML =
    `${nodeData.label || 'Task'} <small class="text-muted">Details</small>`;

    // Set task IDs
    // document.getElementById('task-info-id').textContent = nodeData.taskId || 'Unknown';
    document.getElementById('task-info-run-id').textContent = taskInfo.run_id || 'Unknown';

    // Format status badge
    const status = (taskInfo?.status || nodeData.status || 'pending').toLowerCase();
    const statusColor = statusColors[status] || '#6c757d';
    document.getElementById('task-info-status-badge').innerHTML = `
            <div class="task-status" style="background-color: rgba(${hexToRgb(statusColor)}, 0.2);">
                <span class="task-status-indicator" style="background-color: ${statusColor};"></span>
                <span style="color: ${statusColor};">${capitalizeFirstLetter(status)}</span>
            </div>
        `;

    // Set queue
    document.getElementById('task-info-queue').textContent = taskInfo?.queue || 'default';

    // Set timing info
    document.getElementById('task-info-started').textContent =
    taskInfo?.started_at ? formatDateTime(taskInfo.started_at) : 'Not started';

    document.getElementById('task-info-completed').textContent =
    taskInfo?.completed_at ? formatDateTime(taskInfo.completed_at) : 'Not completed';

    // Calculate duration if both start and end times exist
    if (taskInfo?.started_at && taskInfo?.completed_at) {
    const start = new Date(taskInfo.started_at);
    const end = new Date(taskInfo.completed_at);
    const durationMs = end - start;
    document.getElementById('task-info-duration').textContent = formatDuration(durationMs);
} else if (taskInfo?.started_at && status === 'running') {
    const start = new Date(taskInfo.started_at);
    const now = new Date();
    const durationMs = now - start;
    document.getElementById('task-info-duration').textContent =
    formatDuration(durationMs) + ' (running)';
} else {
    document.getElementById('task-info-duration').textContent = '-';
}

    // Set agent info - this would need to be retrieved from additional API calls
    document.getElementById('task-info-agent').textContent = taskInfo.assigned_agent;

    // Set command
    const commandElement = document.getElementById('task-info-command');
    if (taskInfo?.command) {
    commandElement.textContent = taskInfo.command;
} else {
    commandElement.textContent = 'No command available';
}

    // Set parameters
    const paramsElement = document.getElementById('task-info-parameters');
    if (taskInfo?.parameters && Object.keys(taskInfo.parameters).length > 0) {
    let tableHtml = '<table class="param-table">';
    tableHtml += '<thead><tr><th>Parameter</th><th>Value</th></tr></thead><tbody>';

    for (const [key, value] of Object.entries(taskInfo.parameters)) {
    tableHtml += `<tr>
                    <td>${escapeHtml(key)}</td>
                    <td>${escapeHtml(String(value))}</td>
                </tr>`;
}

    tableHtml += '</tbody></table>';
    paramsElement.innerHTML = tableHtml;
} else {
    paramsElement.innerHTML = '<div class="alert alert-secondary">No parameters available</div>';
}

    // Set environment variables (this would normally come from the task payload)
    const envVarsElement = document.getElementById('task-info-env-vars');

    // For now, we'll show DAG context variables if available
    if (dagData?.context?.variables && Object.keys(dagData.context.variables).length > 0) {
    let tableHtml = '<table class="param-table">';
    tableHtml += '<thead><tr><th>Variable</th><th>Value</th></tr></thead><tbody>';

    for (const [key, value] of Object.entries(dagData.context.variables)) {
    tableHtml += `<tr>
                    <td>${escapeHtml(key)}</td>
                    <td>${escapeHtml(String(value))}</td>
                </tr>`;
}

    tableHtml += '</tbody></table>';
    envVarsElement.innerHTML = tableHtml;
} else {
    envVarsElement.innerHTML = '<div class="alert alert-secondary">No environment variables available</div>';
}

    // Make sure existing accordion items don't trigger panel close
    const accordionButtons = panel.querySelectorAll('.accordion-button');
    accordionButtons.forEach(button => {
    button.addEventListener('click', function(event) {
    event.stopPropagation(); // Prevent closing the panel when clicking accordion buttons
});
});

    // Show panel
    panel.style.display = 'block';
}

    function hideTaskInfo() {
    document.getElementById('task-info-panel').style.display = 'none';
    cy.nodes().removeClass('selected');
    selectedTask = null;
}

    function rerunTask(taskId) {
    // This would call an API endpoint to rerun the task
    alert(`Rerunning task ${taskId} - API endpoint not implemented`);
}

    function viewTaskLogs(taskId) {
    // This would navigate to a logs view page
    alert(`Viewing logs for task ${taskId} - Feature not implemented`);
}

    function startRefreshTimer() {
    if (refreshTimer) clearInterval(refreshTimer);
    refreshCountdown = 10;

    refreshTimer = setInterval(() => {
    refreshCountdown--;
    if (refreshCountdown <= 0) {
    loadDagData();
    resetRefreshTimer();
}
}, 1000);
}

    function resetRefreshTimer() {
    refreshCountdown = 10;
}

    // Utility functions
    function formatDateTime(dateStr) {
    if (!dateStr) return '-';
    const date = new Date(dateStr);
    return date.toLocaleString();
}

    function formatDuration(ms) {
    if (ms < 0) return '-';

    const seconds = Math.floor(ms / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);

    if (hours > 0) {
    return `${hours}h ${minutes % 60}m ${seconds % 60}s`;
} else if (minutes > 0) {
    return `${minutes}m ${seconds % 60}s`;
} else {
    return `${seconds}s`;
}
}

    function hexToRgb(hex) {
    hex = hex.replace('#', '');
    const r = parseInt(hex.substring(0, 2), 16);
    const g = parseInt(hex.substring(2, 4), 16);
    const b = parseInt(hex.substring(4, 6), 16);
    return `${r}, ${g}, ${b}`;
}

    function capitalizeFirstLetter(string) {
    return string.charAt(0).toUpperCase() + string.slice(1);
}

    function escapeHtml(str) {
    if (str === null || str === undefined) return '';
    return str
    .toString()
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}

    window.addEventListener('beforeunload', function () {
    if (refreshTimer) clearInterval(refreshTimer);
});
