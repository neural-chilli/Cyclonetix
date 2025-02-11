document.addEventListener("DOMContentLoaded", function () {
    let cy = cytoscape({
        container: document.getElementById('cy'),
        elements: [
            { data: { id: 'task_1' } },
            { data: { id: 'task_2' } },
            { data: { id: 'task_3' } },
            { data: { id: 'task_4' } },
            { data: { source: 'task_1', target: 'task_2' } },
            { data: { source: 'task_1', target: 'task_3' } },
            { data: { source: 'task_2', target: 'task_4' } },
            { data: { source: 'task_3', target: 'task_4' } },
        ],
        style: [{ selector: 'node', style: { label: 'data(id)' } }],
        layout: { name: 'dagre' }
    });
});
