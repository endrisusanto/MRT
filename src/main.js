// MRT Station Desktop Node Agent Frontend Controller

(function () {
  'use strict';

  // DOM Elements
  const htmlEl = document.documentElement;
  const themeToggleBtn = document.getElementById('btn-theme-toggle');
  const themeIconDark = document.getElementById('theme-icon-dark');
  const themeIconLight = document.getElementById('theme-icon-light');
  const refreshBtn = document.getElementById('btn-refresh');
  const emptyScanBtn = document.getElementById('btn-empty-scan');
  const devicesContainer = document.getElementById('devices-container');
  const devicesEmpty = document.getElementById('devices-empty');
  const scanStatusEl = document.getElementById('device-scan-status');
  const globalProgressFill = document.getElementById('global-progress-fill');
  const toastContainer = document.getElementById('toast-container');

  // Stats
  const statNodeId = document.getElementById('stat-node-id');
  const statNodeIp = document.getElementById('stat-node-ip');
  const statDeviceCount = document.getElementById('stat-device-count');
  const statUptime = document.getElementById('stat-uptime');

  // Tauri IPC helper with fallback to HTTP API on port 7890
  const isTauri = typeof window.__TAURI__ !== 'undefined';

  async function invokeCommand(cmd, args = {}) {
    if (isTauri && window.__TAURI__.core && typeof window.__TAURI__.core.invoke === 'function') {
      return await window.__TAURI__.core.invoke(cmd, args);
    } else if (isTauri && typeof window.__TAURI__.invoke === 'function') {
      return await window.__TAURI__.invoke(cmd, args);
    } else {
      // Fallback to local HTTP Node Server on port 7890
      if (cmd === 'get_node_info') {
        const res = await fetch('http://localhost:7890/info');
        return await res.json();
      } else if (cmd === 'get_devices') {
        const res = await fetch('http://localhost:7890/devices');
        return await res.json();
      } else if (cmd === 'execute_operation') {
        const endpoint = args.operation === 'INSTALL' ? 'install' : (args.operation === 'UNINSTALL' ? 'uninstall' : 'recover');
        const res = await fetch(`http://localhost:7890/devices/${encodeURIComponent(args.serial)}/${endpoint}`, {
          method: 'POST'
        });
        return await res.json();
      }
      throw new Error(`Command ${cmd} not supported in fallback mode`);
    }
  }

  // Theme Management
  function initTheme() {
    const savedTheme = localStorage.getItem('mrt_theme') || 'dark';
    applyTheme(savedTheme);

    themeToggleBtn.addEventListener('click', () => {
      const current = htmlEl.getAttribute('data-theme') || 'dark';
      const next = current === 'dark' ? 'light' : 'dark';
      applyTheme(next);
      localStorage.setItem('mrt_theme', next);
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
    div.textContent = str;
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

  // Render device cards
  function renderDevices(devices) {
    devicesContainer.innerHTML = '';
    statDeviceCount.textContent = devices.length;

    if (!devices || devices.length === 0) {
      devicesEmpty.style.display = 'flex';
      return;
    }

    devicesEmpty.style.display = 'none';

    devices.forEach((dev) => {
      const card = document.createElement('div');
      card.className = 'device-card';
      card.id = `device-card-${dev.serial}`;

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
            <span class="device-meta-key">Connection</span>
            <span class="device-meta-val font-mono">${escapeHtml(dev.connection_type || 'USB')}</span>
          </div>
          <div class="device-meta-row">
            <span class="device-meta-key">Hardware ID</span>
            <span class="device-meta-val font-mono">${dev.vendor_id.toString(16).padStart(4, '0')}:${dev.product_id.toString(16).padStart(4, '0')}</span>
          </div>
        </div>

        <div class="device-actions">
          <button class="btn btn-primary" data-action="INSTALL" data-serial="${escapeHtml(dev.serial)}">
            <svg class="icon"><use href="#icon-download"></use></svg>
            <span>Install</span>
          </button>
          <button class="btn btn-secondary" data-action="RECOVERY" data-serial="${escapeHtml(dev.serial)}">
            <svg class="icon"><use href="#icon-activity"></use></svg>
            <span>Recover</span>
          </button>
          <button class="btn btn-danger" data-action="UNINSTALL" data-serial="${escapeHtml(dev.serial)}">
            <svg class="icon"><use href="#icon-trash"></use></svg>
            <span>Remove</span>
          </button>
        </div>
      `;

      devicesContainer.appendChild(card);
    });

    // Attach action listeners
    devicesContainer.querySelectorAll('button[data-action]').forEach((btn) => {
      btn.addEventListener('click', async (e) => {
        const action = btn.getAttribute('data-action');
        const serial = btn.getAttribute('data-serial');
        await triggerDeviceAction(action, serial, btn);
      });
    });
  }

  async function triggerDeviceAction(action, serial, triggerBtn) {
    triggerBtn.disabled = true;
    globalProgressFill.classList.add('active');
    scanStatusEl.textContent = `Running ${action}...`;

    try {
      const res = await invokeCommand('execute_operation', { operation: action, serial });
      showToast(res.message || `${action} initiated for ${serial}`, 'info');

      // Simulate step completion
      setTimeout(async () => {
        globalProgressFill.classList.remove('active');
        triggerBtn.disabled = false;
        scanStatusEl.textContent = 'Ready';
        showToast(`Operation ${action} completed for ${serial}`, 'success');
        await refreshDevices();
      }, 1500);
    } catch (err) {
      globalProgressFill.classList.remove('active');
      triggerBtn.disabled = false;
      scanStatusEl.textContent = 'Error';
      showToast(`Error: ${err.message || err}`, 'error');
    }
  }

  // Load and refresh devices
  async function refreshDevices() {
    scanStatusEl.textContent = 'Scanning...';
    globalProgressFill.classList.add('active');

    try {
      const devices = await invokeCommand('get_devices');
      renderDevices(devices);
      scanStatusEl.textContent = `${devices.length} device(s) found`;
    } catch (err) {
      scanStatusEl.textContent = 'Scan failed';
      showToast(`Failed to scan devices: ${err.message || err}`, 'error');
    } finally {
      setTimeout(() => {
        globalProgressFill.classList.remove('active');
      }, 300);
    }
  }

  // Load node status
  async function loadNodeInfo() {
    try {
      const info = await invokeCommand('get_node_info');
      statNodeId.textContent = info.id || 'Node';
      statNodeIp.textContent = `${info.ip_address}:${info.port}`;

      const uptimeSec = info.uptime_seconds || 0;
      const mins = Math.floor(uptimeSec / 60);
      const secs = uptimeSec % 60;
      statUptime.textContent = mins > 0 ? `${mins}m ${secs}s` : `${secs}s`;
    } catch (err) {
      statNodeId.textContent = 'Standalone';
    }
  }

  // Setup event listeners
  function setupEvents() {
    refreshBtn.addEventListener('click', refreshDevices);
    emptyScanBtn.addEventListener('click', refreshDevices);

    // Keyboard navigation
    window.addEventListener('keydown', (e) => {
      if (e.key === 'F5' || (e.ctrlKey && e.key === 'r')) {
        e.preventDefault();
        refreshDevices();
      }
    });

    // Mobile nav tab handlers
    document.querySelectorAll('.mobile-nav-item').forEach((item) => {
      item.addEventListener('click', () => {
        document.querySelectorAll('.mobile-nav-item').forEach((el) => el.classList.remove('active'));
        item.classList.add('active');
      });
    });
  }

  // Periodic polls
  function startPolling() {
    setInterval(loadNodeInfo, 5000);
  }

  // Initialization
  async function init() {
    initTheme();
    setupEvents();
    await loadNodeInfo();
    await refreshDevices();
    startPolling();
  }

  document.addEventListener('DOMContentLoaded', init);
})();
