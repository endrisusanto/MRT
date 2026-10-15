// MRT Central Web Dashboard Controller

(function () {
  'use strict';

  // DOM Elements
  const htmlEl = document.documentElement;
  const themeToggleBtn = document.getElementById('btn-theme-toggle');
  const themeIconDark = document.getElementById('theme-icon-dark');
  const themeIconLight = document.getElementById('theme-icon-light');
  const refreshBtn = document.getElementById('btn-refresh');
  const globalProgressFill = document.getElementById('global-progress-fill');
  const toastContainer = document.getElementById('toast-container');

  // Tabs & Views
  const tabDevices = document.getElementById('tab-devices');
  const tabNodes = document.getElementById('tab-nodes');
  const viewDevices = document.getElementById('view-devices');
  const viewNodes = document.getElementById('view-nodes');

  // Containers
  const devicesContainer = document.getElementById('dashboard-devices-container');
  const devicesEmpty = document.getElementById('dashboard-devices-empty');
  const nodesContainer = document.getElementById('dashboard-nodes-container');
  const nodesEmpty = document.getElementById('dashboard-nodes-empty');

  // Stats
  const statActiveNodes = document.getElementById('stat-active-nodes');
  const statTotalDevices = document.getElementById('stat-total-devices');

  // Mobile nav
  const navItemDevices = document.getElementById('nav-item-devices');
  const navItemNodes = document.getElementById('nav-item-nodes');

  // Theme Management
  function initTheme() {
    const savedTheme = localStorage.getItem('mrt_dashboard_theme') || 'dark';
    applyTheme(savedTheme);

    themeToggleBtn.addEventListener('click', () => {
      const current = htmlEl.getAttribute('data-theme') || 'dark';
      const next = current === 'dark' ? 'light' : 'dark';
      applyTheme(next);
      localStorage.setItem('mrt_dashboard_theme', next);
    });
  }

  function applyTheme(theme) {
    htmlEl.setAttribute('data-theme', theme);
    if (theme === 'dark') {
      themeIconDark.style.display = 'inline-block';
      themeIconLight.style.display = 'none';
    } else {
      themeIconDark.style.display = 'none';
      themeIconLight.style.display = 'inline-block';
    }
  }

  // Toast notifications
  function showToast(message, type = 'info') {
    const toast = document.createElement('div');
    toast.className = 'toast';
    toast.innerHTML = `<span class="toast-message">${escapeHtml(message)}</span>`;
    toastContainer.appendChild(toast);

    setTimeout(() => {
      toast.style.opacity = '0';
      toast.style.transform = 'translateY(8px)';
      toast.style.transition = 'opacity 200ms ease, transform 200ms ease';
      setTimeout(() => toast.remove(), 250);
    }, 3500);
  }

  function escapeHtml(str) {
    const div = document.createElement('div');
    div.textContent = str || '';
    return div.innerHTML;
  }

  // Ring progress SVG calculation
  function createRingProgress(percent) {
    const radius = 15;
    const circumference = 2 * Math.PI * radius;
    const offset = circumference - (percent / 100) * circumference;

    return `
      <div class="ring-progress-wrapper" title="Battery: ${percent}%">
        <svg class="ring-svg" viewBox="0 0 38 38">
          <circle class="ring-bg" cx="19" cy="19" r="${radius}"></circle>
          <circle class="ring-bar" cx="19" cy="19" r="${radius}"
                  stroke-dasharray="${circumference}"
                  stroke-dashoffset="${offset}"></circle>
        </svg>
        <span class="ring-text">${percent}%</span>
      </div>
    `;
  }

  // Switch Views
  function showView(view) {
    if (view === 'devices') {
      viewDevices.style.display = 'block';
      viewNodes.style.display = 'none';
      tabDevices.classList.add('active');
      tabNodes.classList.remove('active');
      if (navItemDevices) navItemDevices.classList.add('active');
      if (navItemNodes) navItemNodes.classList.remove('active');
    } else {
      viewDevices.style.display = 'none';
      viewNodes.style.display = 'block';
      tabDevices.classList.remove('active');
      tabNodes.classList.add('active');
      if (navItemDevices) navItemDevices.classList.remove('active');
      if (navItemNodes) navItemNodes.classList.add('active');
    }
  }

  // Fetch and Render Data
  async function loadDashboardData() {
    globalProgressFill.classList.add('active');

    try {
      const [nodesRes, devicesRes] = await Promise.all([
        fetch('/api/nodes'),
        fetch('/api/devices')
      ]);

      const nodes = await nodesRes.json();
      const devices = await devicesRes.json();

      renderNodes(nodes);
      renderDevices(devices);
    } catch (err) {
      showToast(`Failed to update dashboard data: ${err.message}`, 'error');
    } finally {
      setTimeout(() => {
        globalProgressFill.classList.remove('active');
      }, 300);
    }
  }

  function renderNodes(nodes) {
    nodesContainer.innerHTML = '';
    const onlineNodes = nodes.filter(n => n.status === 'ONLINE');
    statActiveNodes.textContent = onlineNodes.length;

    if (!nodes || nodes.length === 0) {
      nodesEmpty.style.display = 'flex';
      return;
    }

    nodesEmpty.style.display = 'none';

    nodes.forEach(node => {
      const card = document.createElement('div');
      card.className = 'node-card';
      const isOnline = node.status === 'ONLINE';

      card.innerHTML = `
        <div class="node-card-header">
          <span class="node-card-title">${escapeHtml(node.name)}</span>
          <span class="badge ${isOnline ? 'badge-installed' : 'badge-missing'}">
            <span class="badge-dot"></span>
            <span>${node.status}</span>
          </span>
        </div>
        <div class="device-meta">
          <div class="device-meta-row">
            <span class="device-meta-key">Node ID</span>
            <span class="device-meta-val font-mono">${escapeHtml(node.id)}</span>
          </div>
          <div class="device-meta-row">
            <span class="device-meta-key">Address</span>
            <span class="device-meta-val font-mono">${escapeHtml(node.ip)}:${node.port}</span>
          </div>
          <div class="device-meta-row">
            <span class="device-meta-key">Devices</span>
            <span class="device-meta-val">${node.devices ? node.devices.length : 0} Connected</span>
          </div>
          <div class="device-meta-row">
            <span class="device-meta-key">Last Heartbeat</span>
            <span class="device-meta-val">${node.secondsAgo}s ago</span>
          </div>
        </div>
      `;

      nodesContainer.appendChild(card);
    });
  }

  function renderDevices(devices) {
    devicesContainer.innerHTML = '';
    statTotalDevices.textContent = devices.length;

    if (!devices || devices.length === 0) {
      devicesEmpty.style.display = 'flex';
      return;
    }

    devicesEmpty.style.display = 'none';

    devices.forEach(dev => {
      const card = document.createElement('div');
      card.className = 'device-card';
      const isInstalled = dev.agent_status === 'INSTALLED';
      const badgeClass = isInstalled ? 'badge-installed' : (dev.agent_status === 'UPDATING' ? 'badge-updating' : 'badge-missing');
      const badgeText = isInstalled ? 'Installed' : (dev.agent_status === 'UPDATING' ? 'Updating' : 'Not Installed');
      const battery = dev.battery_level != null ? dev.battery_level : 100;

      card.innerHTML = `
        <div class="device-header">
          <div class="device-info">
            <span class="device-model">${escapeHtml(dev.model || 'Samsung Device')}</span>
            <span class="device-serial font-mono">${escapeHtml(dev.serial)}</span>
          </div>
          ${createRingProgress(battery)}
        </div>

        <div>
          <span class="badge ${badgeClass}">
            <span class="badge-dot"></span>
            <span>${badgeText}</span>
          </span>
        </div>

        <div class="device-meta">
          <div class="device-meta-row">
            <span class="device-meta-key">Reporting Node</span>
            <span class="device-meta-val font-mono">${escapeHtml(dev.node_name)}</span>
          </div>
          <div class="device-meta-row">
            <span class="device-meta-key">Connection</span>
            <span class="device-meta-val font-mono">${escapeHtml(dev.connection_type || 'USB')}</span>
          </div>
        </div>

        <div class="device-actions">
          <button class="btn btn-primary" data-action="INSTALL" data-serial="${escapeHtml(dev.serial)}" data-node-ip="${escapeHtml(dev.node_ip)}" data-node-port="${dev.node_port}">
            <svg class="icon"><use href="#icon-download"></use></svg>
            <span>Install</span>
          </button>
          <button class="btn btn-secondary" data-action="RECOVER" data-serial="${escapeHtml(dev.serial)}" data-node-ip="${escapeHtml(dev.node_ip)}" data-node-port="${dev.node_port}">
            <svg class="icon"><use href="#icon-activity"></use></svg>
            <span>Recover</span>
          </button>
          <button class="btn btn-danger" data-action="UNINSTALL" data-serial="${escapeHtml(dev.serial)}" data-node-ip="${escapeHtml(dev.node_ip)}" data-node-port="${dev.node_port}">
            <svg class="icon"><use href="#icon-trash"></use></svg>
            <span>Remove</span>
          </button>
        </div>
      `;

      devicesContainer.appendChild(card);
    });

    // Attach actions
    devicesContainer.querySelectorAll('button[data-action]').forEach(btn => {
      btn.addEventListener('click', async () => {
        const action = btn.getAttribute('data-action');
        const serial = btn.getAttribute('data-serial');
        const node_ip = btn.getAttribute('data-node-ip');
        const node_port = btn.getAttribute('data-node-port');

        btn.disabled = true;
        globalProgressFill.classList.add('active');

        try {
          const res = await fetch('/api/operations', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
              node_ip,
              node_port: parseInt(node_port, 10),
              serial,
              operation: action
            })
          });

          const data = await res.json();
          showToast(data.message || `Dispatched ${action} on ${serial}`, 'info');
        } catch (err) {
          showToast(`Operation failed: ${err.message}`, 'error');
        } finally {
          setTimeout(() => {
            btn.disabled = false;
            globalProgressFill.classList.remove('active');
            loadDashboardData();
          }, 1500);
        }
      });
    });
  }

  // WebSocket Connection
  function setupWebSocket() {
    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = `${proto}//${location.host}/ws`;
    let ws = null;

    function connect() {
      try {
        ws = new WebSocket(wsUrl);

        ws.onopen = () => {
          console.log('[MRT Hub WS] Connected');
        };

        ws.onmessage = (event) => {
          try {
            const data = JSON.parse(event.data);
            if (data.event === 'node_updated' || data.event === 'operation_proxied') {
              loadDashboardData();
            }
          } catch (e) {
            // Non-JSON ping
          }
        };

        ws.onclose = () => {
          console.log('[MRT Hub WS] Closed, retrying in 3s...');
          setTimeout(connect, 3000);
        };

        ws.onerror = () => {
          ws.close();
        };
      } catch (err) {
        setTimeout(connect, 5000);
      }
    }

    connect();
  }

  // Event Listeners
  function setupEvents() {
    tabDevices.addEventListener('click', () => showView('devices'));
    tabNodes.addEventListener('click', () => showView('nodes'));

    if (navItemDevices) navItemDevices.addEventListener('click', () => showView('devices'));
    if (navItemNodes) navItemNodes.addEventListener('click', () => showView('nodes'));

    refreshBtn.addEventListener('click', loadDashboardData);

    window.addEventListener('keydown', (e) => {
      if (e.key === 'F5' || (e.ctrlKey && e.key === 'r')) {
        e.preventDefault();
        loadDashboardData();
      }
    });
  }

  // Initialize
  async function init() {
    initTheme();
    setupEvents();
    setupWebSocket();
    await loadDashboardData();
    setInterval(loadDashboardData, 10000); // 10s poll fallback
  }

  document.addEventListener('DOMContentLoaded', init);
})();
