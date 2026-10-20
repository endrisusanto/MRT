// MRT Central Web Dashboard - Fleet Diagnostics & Token Controller
// Octopus-styled Design Language & Live Terminal Integration

(function () {
  'use strict';

  // State
  let allDevices = [];
  let allNodes = [];
  const selectedSerials = new Set();
  const selectedModes = new Set([0, 1, 12, 49]);
  let filterSearch = '';
  let filterModel = 'all';
  let filterNode = 'all';

  // In-row progress state: { [serial]: { pct: number, step: string } }
  const deviceProgress = {};

  // Logs buffer: Array<{ timestamp: string, serial: string, level: string, message: string }>
  const logs = [];
  let activeLogSerial = null;

  // DOM Elements
  const htmlEl = document.documentElement;
  const themeToggleBtn = document.getElementById('btn-theme-toggle');
  const themeIconDark = document.getElementById('theme-icon-dark');
  const themeIconLight = document.getElementById('theme-icon-light');
  const refreshBtn = document.getElementById('btn-refresh');
  const toastContainer = document.getElementById('toast-container');

  // Stat Pills
  const statPillNodes = document.getElementById('stat-active-nodes');
  const statPillDevices = document.getElementById('stat-total-devices');
  const statPillTokens = document.getElementById('stat-token-count');

  // Terminal Drawer
  const btnOpenTerminal = document.getElementById('btn-open-terminal');
  const drawerBackdrop = document.getElementById('drawer-backdrop');
  const btnCloseTerminal = document.getElementById('btn-close-drawer') || document.getElementById('btn-close-terminal');
  const btnClearTerminal = document.getElementById('btn-clear-logs') || document.getElementById('btn-clear-terminal');
  const terminalBox = document.getElementById('terminal-box');
  const terminalTarget = document.getElementById('drawer-subtitle') || document.getElementById('terminal-active-target');

  // Tabs & Views
  const tabDevices = document.getElementById('tab-devices');
  const tabNodes = document.getElementById('tab-nodes');
  const viewDevices = document.getElementById('view-devices');
  const viewNodes = document.getElementById('view-nodes');
  const navItemDevices = document.getElementById('nav-item-devices');
  const navItemNodes = document.getElementById('nav-item-nodes');

  // Accordion Elements
  const accordion = document.getElementById('accordion-target-workflow');
  const accordionToggle = document.getElementById('btn-toggle-accordion');
  const modelRecapChips = document.getElementById('model-recap-chips');
  const badgeSelectedCount = document.getElementById('badge-selected-count');
  const statSelectedCount = document.getElementById('stat-selected-count');
  const statSelectedModels = document.getElementById('stat-selected-models');
  const btnToggleSelectAll = document.getElementById('btn-toggle-select-all');
  const iconSelectState = document.getElementById('icon-select-state');
  const labelSelectState = document.getElementById('label-select-state');
  const btnBatchInstall = document.getElementById('btn-batch-install');
  const btnBatchRemove = document.getElementById('btn-batch-remove');
  const masterCheckbox = document.getElementById('master-checkbox');
  const fleetTableBody = document.getElementById('fleet-table-body');
  const fleetTableEmpty = document.getElementById('fleet-table-empty');

  // Filter Elements
  const inputSearch = document.getElementById('filter-search');
  const selectModel = document.getElementById('filter-model');
  const selectNode = document.getElementById('filter-node');

  // Node stats view
  const statActiveNodes = document.getElementById('stat-active-nodes');
  const statTotalDevices = document.getElementById('stat-total-devices');
  const nodesContainer = document.getElementById('dashboard-nodes-container');
  const nodesEmpty = document.getElementById('dashboard-nodes-empty');

  // --- Theme Management ---
  function initTheme() {
    const savedTheme = localStorage.getItem('mrt_theme') || 'dark';
    applyTheme(savedTheme);

    if (themeToggleBtn) {
      themeToggleBtn.addEventListener('click', () => {
        const current = htmlEl.getAttribute('data-theme') || 'dark';
        const next = current === 'dark' ? 'light' : 'dark';
        applyTheme(next);
        localStorage.setItem('mrt_theme', next);
      });
    }
  }

  function applyTheme(theme) {
    htmlEl.setAttribute('data-theme', theme);
    if (themeIconDark && themeIconLight) {
      if (theme === 'dark') {
        themeIconDark.style.display = 'inline-block';
        themeIconLight.style.display = 'none';
      } else {
        themeIconDark.style.display = 'none';
        themeIconLight.style.display = 'inline-block';
      }
    }
  }

  // --- Terminal Logging ---
  function formatTime(date = new Date()) {
    const pad = (n) => String(n).padStart(2, '0');
    return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
  }

  function addLog(serial, level, message) {
    const logEntry = {
      timestamp: formatTime(),
      serial: serial || 'HUB',
      level: (level || 'INFO').toUpperCase(),
      message: message || ''
    };
    logs.push(logEntry);
    if (logs.length > 500) logs.shift();
    renderTerminalLogs();
  }

  function renderTerminalLogs() {
    if (!terminalBox) return;
    const filteredLogs = activeLogSerial 
      ? logs.filter(l => l.serial === activeLogSerial || l.serial === 'HUB')
      : logs;

    if (filteredLogs.length === 0) {
      terminalBox.innerHTML = `<div class="terminal-line" style="color:var(--color-text-muted);">// No terminal logs recorded for target ${escapeHtml(activeLogSerial || 'all')}</div>`;
      return;
    }

    terminalBox.innerHTML = filteredLogs.map(l => {
      const levelClass = l.level.toLowerCase();
      return `<div class="terminal-line ${levelClass}">` +
        `<span class="terminal-timestamp">[${l.timestamp}]</span> ` +
        `<span class="terminal-node">[${escapeHtml(l.serial)}]</span> ` +
        `<span class="terminal-level">[${escapeHtml(l.level)}]</span> ` +
        `<span class="terminal-msg">${escapeHtml(l.message)}</span>` +
      `</div>`;
    }).join('');

    terminalBox.scrollTop = terminalBox.scrollHeight;
  }

  function openTerminalDrawer(serial = null) {
    activeLogSerial = serial;
    if (terminalTarget) {
      terminalTarget.textContent = serial ? `Target: ${serial}` : 'Global Fleet Stream';
    }
    if (drawerBackdrop) {
      drawerBackdrop.classList.add('open');
      drawerBackdrop.classList.add('active');
    }
    renderTerminalLogs();
  }

  function closeTerminalDrawer() {
    if (drawerBackdrop) {
      drawerBackdrop.classList.remove('open');
      drawerBackdrop.classList.remove('active');
    }
  }

  // --- In-Row Card Progress ---
  function setDeviceProgress(serial, pct, step) {
    if (pct <= 0 || pct >= 100) {
      if (pct >= 100) {
        deviceProgress[serial] = { pct: 100, step: step || 'Done' };
        renderFleetTable();
        setTimeout(() => {
          delete deviceProgress[serial];
          renderFleetTable();
        }, 1500);
      } else {
        delete deviceProgress[serial];
        renderFleetTable();
      }
    } else {
      deviceProgress[serial] = { pct, step: step || 'In Progress' };
      renderFleetTable();
    }
  }

  // --- Toast ---
  function showToast(message, type = 'info') {
    if (!toastContainer) return;
    const toast = document.createElement('div');
    toast.className = 'toast';
    toast.innerHTML = `<span>${escapeHtml(message)}</span>`;
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

  // --- View Switching ---
  function showView(view) {
    if (view === 'devices') {
      if (viewDevices) viewDevices.style.display = 'block';
      if (viewNodes) viewNodes.style.display = 'none';
      if (tabDevices) tabDevices.classList.add('active');
      if (tabNodes) tabNodes.classList.remove('active');
      if (navItemDevices) navItemDevices.classList.add('active');
      if (navItemNodes) navItemNodes.classList.remove('active');
    } else {
      if (viewDevices) viewDevices.style.display = 'none';
      if (viewNodes) viewNodes.style.display = 'block';
      if (tabDevices) tabDevices.classList.remove('active');
      if (tabNodes) tabNodes.classList.add('active');
      if (navItemDevices) navItemDevices.classList.remove('active');
      if (navItemNodes) navItemNodes.classList.add('active');
    }
  }

  async function tauriInvoke(cmd, args) {
    if (window.__TAURI__) {
      const inv = window.__TAURI__.core?.invoke || window.__TAURI__.invoke;
      if (typeof inv === 'function') {
        try {
          return await inv(cmd, args);
        } catch (e) {
          console.warn(`[Tauri IPC] ${cmd} error:`, e);
        }
      }
    }
    return null;
  }

  // --- Fetch Data ---
  async function fetchData() {
    try {
      const isLocal = isLocalClient();

      // 1. Fetch devices: in local desktop app (Tauri IPC), direct query
      let devices = await tauriInvoke('get_devices');

      if ((!devices || devices.length === 0) && isLocal) {
        try {
          const res = await fetch('http://127.0.0.1:7890/devices');
          if (res.ok) {
            devices = await res.json();
          }
        } catch (e) {}
      }

      if (!devices || devices.length === 0) {
        try {
          const res = await fetch('/api/devices');
          if (res.ok) {
            devices = await res.json();
          }
        } catch (e) {}
      }

      // Fallback for local client
      if ((!devices || devices.length === 0) && isLocal) {
        try {
          const res = await fetch('http://localhost:7890/devices');
          if (res.ok) {
            devices = await res.json();
          }
        } catch (e) {}
      }

      // ponytail: filter out non-Android / non-mobile hardware (e.g. Samsung Portable SSDs, Flash drives, Storage devices)
      allDevices = (devices || []).filter(d => {
        const m = (d.model || '').toUpperCase();
        return !m.includes('PSSD') && !m.includes('SSD') && !m.includes('FLASH') && !m.includes('STORAGE') && !m.includes('HUB');
      });

      // 2. Fetch node info: in local desktop app (Tauri IPC), direct query
      let nodes = [];
      const localNode = await tauriInvoke('get_node_info');
      if (localNode) {
        nodes = [localNode];
        if (localNode.version) {
          const vTag = document.getElementById('app-version-tag');
          if (vTag) vTag.textContent = `v${localNode.version}`;
        }
      }

      if (nodes.length === 0 && isLocal) {
        try {
          const res = await fetch('http://127.0.0.1:7890/info');
          if (res.ok) {
            const info = await res.json();
            nodes = [info];
            if (info.version) {
              const vTag = document.getElementById('app-version-tag');
              if (vTag) vTag.textContent = `v${info.version}`;
            }
          }
        } catch (e) {}
      }

      if (nodes.length === 0) {
        try {
          const res = await fetch('/api/nodes');
          if (res.ok) {
            nodes = await res.json();
            if (nodes.length > 0 && nodes[0].version) {
              const vTag = document.getElementById('app-version-tag');
              if (vTag) vTag.textContent = `v${nodes[0].version}`;
            }
          }
        } catch (e) {}
      }
      allNodes = nodes || [];

      updateStats();
      updateFilterOptions();
      renderFleetTable();
      renderNodes();
    } catch (err) {
      console.error('Fetch error:', err);
      addLog('HUB', 'ERROR', `Connection error: ${err.message || 'Node agent offline'}`);
      showToast('Gagal terhubung ke daemon MRT', 'error');
    }
  }

  // ponytail: calculate days remaining from YYYY-MM-DD expiration string
  function getDaysRemaining(expiresStr) {
    if (!expiresStr || expiresStr === '-' || expiresStr === 'None') return null;
    const parts = expiresStr.split('-');
    if (parts.length === 3) {
      const expDate = new Date(parseInt(parts[0], 10), parseInt(parts[1], 10) - 1, parseInt(parts[2], 10));
      const now = new Date();
      expDate.setHours(0, 0, 0, 0);
      now.setHours(0, 0, 0, 0);
      return Math.round((expDate.getTime() - now.getTime()) / (1000 * 60 * 60 * 24));
    }
    return null;
  }

  function updateStats() {
    let activeCount = 0;
    let warningCount = 0;
    let noneCount = 0;

    for (const d of allDevices) {
      const hwVerified = Boolean(d.hw_verified);
      const isTokenActive = d.token_status === 'ACTIVE' && hwVerified;
      if (isTokenActive) {
        const days = getDaysRemaining(d.token_expires);
        if (days !== null && days < 0) {
          noneCount += 1;
        } else if (days !== null && days <= 14) {
          warningCount += 1;
        } else {
          activeCount += 1;
        }
      } else {
        noneCount += 1;
      }
    }

    if (statPillNodes) statPillNodes.textContent = allNodes.length || 1;
    if (statPillDevices) statPillDevices.textContent = allDevices.length;
    if (statPillTokens) statPillTokens.textContent = activeCount;

    const statWarning = document.getElementById('stat-warning-count');
    if (statWarning) statWarning.textContent = warningCount;
    const statNone = document.getElementById('stat-none-count');
    if (statNone) statNone.textContent = noneCount;

    if (statActiveNodes) statActiveNodes.textContent = allNodes.length || 1;
    if (statTotalDevices) statTotalDevices.textContent = allDevices.length;

    updateSelectionRecap();
  }

  function updateSelectionRecap() {
    const selectedCount = selectedSerials.size;
    if (badgeSelectedCount) badgeSelectedCount.innerHTML = `<span class="count-value">${selectedCount}</span><span class="count-unit"> Unit</span>`;
    if (statSelectedCount) statSelectedCount.textContent = selectedCount;

    const modelStats = {};
    for (const d of allDevices) {
      if (selectedSerials.has(d.serial)) {
        if (!modelStats[d.model]) {
          modelStats[d.model] = { count: 0, active: 0, none: 0 };
        }
        modelStats[d.model].count += 1;
        if (d.token_status === 'ACTIVE' && Boolean(d.hw_verified)) {
          modelStats[d.model].active += 1;
        } else {
          modelStats[d.model].none += 1;
        }
      }
    }

    const modelKeys = Object.keys(modelStats);
    if (modelKeys.length === 0) {
      if (modelRecapChips) modelRecapChips.innerHTML = '';
      if (statSelectedModels) statSelectedModels.textContent = 'None selected';
    } else {
      if (modelRecapChips) {
        modelRecapChips.innerHTML = modelKeys.map(m => {
          const s = modelStats[m];
          const hasActive = s.active > 0;
          const statusClass = hasActive ? 'active' : 'none';
          const iconId = hasActive ? '#icon-check-circle' : '#icon-x-circle';
          return `
            <span class="chip-model-pill ${statusClass}" title="${escapeHtml(m)}: ${s.count} Unit (${s.active} Active, ${s.none} No Token)">
              <svg class="chip-icon"><use href="${iconId}"></use></svg>
              <span class="chip-model-name">${escapeHtml(m)}</span>
              <span class="chip-model-count">${s.count}</span>
            </span>
          `;
        }).join('');
      }
      if (statSelectedModels) {
        statSelectedModels.textContent = modelKeys
          .map(m => `${m} (${modelStats[m].count})`)
          .join(', ');
      }
    }

    // Update dynamic select all / deselect all button state
    if (labelSelectState && iconSelectState) {
      const totalDevices = allDevices.length;
      const isAllSelected = totalDevices > 0 && selectedCount === totalDevices;
      if (isAllSelected) {
        labelSelectState.textContent = 'Batal Pilih';
        iconSelectState.innerHTML = '<use href="#icon-check"></use>';
        if (btnToggleSelectAll) btnToggleSelectAll.title = 'Batalkan Pilihan Semua Perangkat';
      } else {
        labelSelectState.textContent = 'Pilih Semua';
        iconSelectState.innerHTML = '<use href="#icon-check-check"></use>';
        if (btnToggleSelectAll) btnToggleSelectAll.title = 'Pilih Semua Perangkat';
      }
    }
  }

  function updateFilterOptions() {
    if (!selectNode) return;
    const currentNode = selectNode.value || 'all';

    // Count devices per workstation node
    const nodeCounts = {};
    for (const d of allDevices) {
      const nKey = d.node_id || d.node_name || (allNodes[0]?.id) || 'local';
      nodeCounts[nKey] = (nodeCounts[nKey] || 0) + 1;
    }

    const totalNodes = allNodes.length > 0 ? allNodes.length : 1;
    const options = [`<option value="all">Semua Workstation PC (${totalNodes})</option>`];

    if (allNodes.length > 0) {
      allNodes.forEach(n => {
        const id = n.id || n.name || 'local';
        const name = n.name || n.id || 'Workstation';
        const count = nodeCounts[id] || n.device_count || 0;
        options.push(`<option value="${escapeHtml(id)}">${escapeHtml(name)} (${count} Perangkat)</option>`);
      });
    } else {
      const localCount = allDevices.length;
      options.push(`<option value="local">Local Workstation (${localCount} Perangkat)</option>`);
    }

    selectNode.innerHTML = options.join('');
    if (Array.from(selectNode.options).some(o => o.value === currentNode)) {
      selectNode.value = currentNode;
    }
  }

  function getFilteredDevices() {
    return allDevices.filter(d => {
      if (filterNode !== 'all') {
        const devNode = d.node_id || d.node_name || 'local';
        if (devNode !== filterNode && d.node_id !== filterNode && d.node_name !== filterNode) {
          return false;
        }
      }
      if (filterSearch) {
        const q = filterSearch.toLowerCase();
        const s = (d.serial || '').toLowerCase();
        const m = (d.model || '').toLowerCase();
        const p = (d.port_name || '').toLowerCase();
        const u = (d.token_owner || '').toLowerCase();
        const n = (d.node_name || d.node_id || '').toLowerCase();
        if (!s.includes(q) && !m.includes(q) && !p.includes(q) && !u.includes(q) && !n.includes(q)) return false;
      }
      return true;
    });
  }

  function renderFleetTable() {
    if (!fleetTableBody) return;
    const filtered = getFilteredDevices();
    const fleetTableContainer = document.getElementById('fleet-table-container');

    if (filtered.length === 0) {
      fleetTableBody.innerHTML = '';
      if (fleetTableEmpty) fleetTableEmpty.style.display = 'flex';
      if (fleetTableContainer) {
        fleetTableContainer.style.display = 'none';
        fleetTableContainer.scrollLeft = 0;
      }
      return;
    }

    if (fleetTableEmpty) fleetTableEmpty.style.display = 'none';
    if (fleetTableContainer) fleetTableContainer.style.display = 'block';
    fleetTableBody.innerHTML = filtered.map(d => {
      const isSelected = selectedSerials.has(d.serial);
      const hwVerified = Boolean(d.hw_verified);
      const isTokenActive = d.token_status === 'ACTIVE' && hwVerified;
      const progress = deviceProgress[d.serial];

      // Active mode badges (only displayed if confirmed active in TrustZone)
      const activeModes = isTokenActive ? (d.active_modes || []) : [];
      const badges = [];
      if (activeModes.some(m => m.includes('ENG_KERNEL'))) {
        badges.push('<span class="mini-mode-badge badge-eng" title="MODE_ENG_KERNEL (0)">ENG</span>');
      }
      if (activeModes.some(m => m.includes('SKIP_SUW') || m.includes('INIT_EM'))) {
        badges.push('<span class="mini-mode-badge badge-em" title="MODE_SKIP_SUW (1)">SUW</span>');
      }
      if (activeModes.some(m => m.includes('SCREENSHORT') || m.includes('SCREENSHOT') || m.includes('ACCESS_SOD') || m.includes('RAM_DUMP'))) {
        badges.push('<span class="mini-mode-badge badge-sod" title="MODE_ALLOW_GET_GA_SCREENSHORT (12/49)">GA_SS</span>');
      }
      const modesHtml = badges.length > 0 ? badges.join('') : '<span style="color:var(--color-text-muted);font-size:10px;">None</span>';

      // HW Verification badge (Solid: HW OK or UNVERIFIED, no cache)
      let hwBadgeHtml;
      if (isTokenActive && hwVerified) {
        hwBadgeHtml = `<span class="token-status-pill active" style="font-size:10px;padding:2px 8px;" title="Token dikonfirmasi langsung dari modem hardware TrustZone">
          <svg class="badge-icon"><use href="#icon-check-circle"></use></svg> HW OK
        </span>`;
      } else {
        hwBadgeHtml = `<span class="token-status-pill none" style="font-size:10px;padding:2px 8px;" title="Belum terpasang di hardware modem">
          <svg class="badge-icon"><use href="#icon-x-circle"></use></svg> UNVERIFIED
        </span>`;
      }

      // Days remaining & expiration display
      const days = getDaysRemaining(d.token_expires);
      let statusHtml;
      let expireHtml;

      if (isTokenActive) {
        if (days !== null && days < 0) {
          statusHtml = `
            <span class="token-status-pill expired">
              <svg class="badge-icon"><use href="#icon-x-circle"></use></svg>
              Expired
            </span>
          `;
          expireHtml = `
            <div class="device-expire-pill">
              <span class="device-expire-date">${escapeHtml(d.token_expires || '-')}</span>
              <span class="device-countdown-badge expired">Expired</span>
            </div>
          `;
        } else if (days !== null && days <= 14) {
          statusHtml = `
            <span class="token-status-pill warning" title="Masa berlaku tinggal ${days} hari">
              <svg class="badge-icon"><use href="#icon-alert-triangle"></use></svg>
              Segera Habis
            </span>
          `;
          expireHtml = `
            <div class="device-expire-pill">
              <span class="device-expire-date">${escapeHtml(d.token_expires || '-')}</span>
              <span class="device-countdown-badge warning">${days} hari lagi</span>
            </div>
          `;
        } else {
          const daysText = days !== null ? `${days} hari lagi` : 'Aktif';
          statusHtml = `
            <span class="token-status-pill active" title="Owner: ${escapeHtml(d.token_owner || 'user')}">
              <svg class="badge-icon"><use href="#icon-check-circle"></use></svg>
              Aktif
            </span>
          `;
          expireHtml = `
            <div class="device-expire-pill">
              <span class="device-expire-date">${escapeHtml(d.token_expires || '-')}</span>
              <span class="device-countdown-badge active">${daysText}</span>
            </div>
          `;
        }
      } else if (d.token_status === 'EXPIRED') {
        statusHtml = `
          <span class="token-status-pill expired">
            <svg class="badge-icon"><use href="#icon-alert-triangle"></use></svg>
            Expired
          </span>
        `;
        expireHtml = `<span class="device-expire-date">${escapeHtml(d.token_expires || '-')}</span>`;
      } else {
        statusHtml = `
          <span class="token-status-pill none">
            <svg class="badge-icon"><use href="#icon-x-circle"></use></svg>
            No Token
          </span>
        `;
        expireHtml = '<span class="device-expire-muted">-</span>';
      }

      // In-row progress indicator (Octopus continuous left-to-right bar + metric text)
      let rowStyle = '';
      let progressMetricHtml = '';
      let progressBarRowHtml = '';
      if (progress) {
        const pct = progress.pct || 0;
        rowStyle = `--progress-pct: ${pct}%;`;
        progressMetricHtml = `<span class="octopus-progress-metric">${escapeHtml(progress.step || 'RUN')}: ${pct}%</span>`;
        statusHtml = `
          <span class="token-status-pill flashing" title="${escapeHtml(progress.step)} (${pct}%)">
            <svg class="badge-icon icon-spin"><use href="#icon-refresh"></use></svg>
            ${pct}%
          </span>
        `;
        progressBarRowHtml = `
          <tr class="device-progress-track-row" data-serial="${escapeHtml(d.serial)}">
            <td colspan="10" class="device-progress-track-cell">
              <div class="device-progress-fill-bar" style="width: ${pct}%;"></div>
            </td>
          </tr>
        `;
      }

      return `
        <tr class="${isSelected ? 'selected' : ''} ${progress ? 'flashing-row' : ''}" style="${rowStyle}" data-serial="${escapeHtml(d.serial)}">
          <td style="text-align: center;">
            <input type="checkbox" class="device-checkbox" data-serial="${escapeHtml(d.serial)}" ${isSelected ? 'checked' : ''} ${progress ? 'disabled' : ''}>
          </td>
          <td><span class="device-port">${escapeHtml(d.port_name || 'USB')}</span></td>
          <td><span class="device-model">${escapeHtml(d.model)}</span></td>
          <td>
            <span class="device-serial">${escapeHtml(d.serial)}</span>
          </td>
          <td style="text-align: center;">${statusHtml}</td>
          <td style="text-align: center;"><span class="device-owner-tag">${escapeHtml(d.token_owner || '-')}</span></td>
          <td>${expireHtml}</td>
          <td><div class="active-mode-badges">${modesHtml}</div></td>
          <td style="text-align: center;">${hwBadgeHtml}</td>
          <td style="text-align: right;">
            <div style="display: inline-flex; gap: 6px; align-items: center; justify-content: flex-end;">
              ${progressMetricHtml}
              <button class="btn btn-secondary btn-sm btn-query-single" data-serial="${escapeHtml(d.serial)}" ${progress ? 'disabled' : ''} title="Periksa status token via AT+ENGMODES=9,0">
                Periksa
              </button>
              <button class="btn btn-danger btn-sm btn-remove-single" data-serial="${escapeHtml(d.serial)}" ${progress ? 'disabled' : ''} title="Cabut token offline via modem AT">
                Hapus
              </button>
            </div>
          </td>
        </tr>
        ${progressBarRowHtml}
      `;
    }).join('');

    // Update master checkbox state
    if (masterCheckbox) {
      const allFilteredSelected = filtered.length > 0 && filtered.every(d => selectedSerials.has(d.serial));
      masterCheckbox.checked = allFilteredSelected;
    }

    attachTableEvents();
  }

  function attachTableEvents() {
    // Individual device checkboxes
    document.querySelectorAll('.device-checkbox').forEach(cb => {
      cb.addEventListener('change', (e) => {
        const serial = e.target.getAttribute('data-serial');
        if (e.target.checked) {
          selectedSerials.add(serial);
        } else {
          selectedSerials.delete(serial);
        }
        updateSelectionRecap();
        renderFleetTable();
      });
    });

    // Single query / periksa button
    document.querySelectorAll('.btn-query-single').forEach(btn => {
      btn.addEventListener('click', (e) => {
        const serial = e.target.getAttribute('data-serial');
        queryDevices([serial]);
      });
    });

    // Single remove button
    document.querySelectorAll('.btn-remove-single').forEach(btn => {
      btn.addEventListener('click', (e) => {
        const serial = e.target.getAttribute('data-serial');
        removeTokenOffline([serial]);
      });
    });
  }

  function renderNodes() {
    if (!nodesContainer) return;
    if (allNodes.length === 0) {
      nodesContainer.innerHTML = '';
      if (nodesEmpty) nodesEmpty.style.display = 'flex';
      return;
    }
    if (nodesEmpty) nodesEmpty.style.display = 'none';
    nodesContainer.innerHTML = allNodes.map(n => {
      const isRelay = Boolean(n.is_relay);
      const relayBadge = isRelay
        ? '<span class="status-badge" style="background:rgba(59,130,246,0.15);color:#60a5fa;border:1px solid rgba(59,130,246,0.3);font-size:10px;padding:2px 6px;">Relay Agent</span>'
        : '';
      const ip = n.ip_address || n.ip || '127.0.0.1';
      const devCount = n.device_count || (Array.isArray(n.devices) ? n.devices.length : 0);
      return `
      <div class="node-stat-card">
        <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:4px;">
          <span class="stat-label">Agent Node</span>
          ${relayBadge}
        </div>
        <span class="stat-value font-mono">${escapeHtml(n.name || n.id)}</span>
        <span class="stat-sub font-mono">${escapeHtml(ip)}:${n.port || 7890}</span>
        <div style="margin-top: 6px; font-size: 11px; color: var(--color-green);">Online • ${devCount} Devices Attached</div>
      </div>
    `;}).join('');
  }

  function isLocalClient() {
    return Boolean(window.__TAURI__) || window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1' || window.location.hostname.includes('tauri');
  }

  function getNodeBaseUrl(serial) {
    if (serial) {
      const dev = allDevices.find(d => d.serial === serial);
      if (dev && dev.node_ip && dev.node_ip !== '127.0.0.1' && dev.node_ip !== 'localhost' && isLocalClient()) {
        return `http://${dev.node_ip}:${dev.node_port || 7890}`;
      }
    }
    return isLocalClient() ? 'http://127.0.0.1:7890' : '';
  }

  // --- Operations with In-Row Progress & Live Terminal Logging ---
  async function queryDevices(serials) {
    if (!serials || serials.length === 0) {
      showToast('Pilih setidaknya 1 perangkat untuk periksa status token', 'error');
      return;
    }

    showToast(`Memeriksa status token TrustZone hardware pada ${serials.length} unit...`, 'info');

    for (const serial of serials) {
      setDeviceProgress(serial, 25, 'Probing Modem');
      addLog(serial, 'CMD', `Opening modem COM port for TrustZone verification`);

      try {
        setDeviceProgress(serial, 65, 'AT+ENGMODES=9,0');
        addLog(serial, 'CMD', `AT+ENGMODES=9,0 (Direct SE Query)`);

        let data = null;
        if (window.__TAURI__) {
          data = await tauriInvoke('execute_operation', { operation: 'QUERY', serial });
        } else {
          const baseUrl = getNodeBaseUrl(serial);
          const res = await fetch(`${baseUrl}/devices/${encodeURIComponent(serial)}/query`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' }
          });
          if (res.ok) {
            data = await res.json().catch(() => ({}));
          }
        }

        if (data && data.status !== 'FAILED') {
          setDeviceProgress(serial, 100, 'Checked OK');
          addLog(serial, 'INFO', data.message || `Status token terverifikasi di hardware`);
          showToast(`Selesai di ${serial}: ${data.message || 'Checked OK'}`, 'info');
        } else {
          setDeviceProgress(serial, 100, 'No Token');
          addLog(serial, 'WARN', data ? data.message : 'Gagal membaca status modem');
        }
      } catch (e) {
        setDeviceProgress(serial, 100, 'Error');
        addLog(serial, 'ERROR', `Gagal koneksi saat verifikasi token: ${e.message}`);
      }
    }

    setTimeout(fetchData, 600);
  }

  async function removeTokenOffline(serials) {
    if (!serials || serials.length === 0) {
      showToast('Pilih setidaknya 1 perangkat untuk hapus token offline', 'error');
      return;
    }

    showToast(`Mengirim perintah AT+ENGMODES=0,0,3,0 ke ${serials.length} unit...`, 'info');

    for (const serial of serials) {
      setDeviceProgress(serial, 25, 'Connecting AT port');
      addLog(serial, 'CMD', `Opening serial device port for ${serial}`);

      try {
        setDeviceProgress(serial, 65, 'Executing AT+ENGMODES=0,0,3,0');
        addLog(serial, 'CMD', `Sending raw AT string: AT+ENGMODES=0,0,3,0\\r\\n`);

        const baseUrl = getNodeBaseUrl(serial);
        const res = await fetch(`${baseUrl}/devices/${encodeURIComponent(serial)}/remove-token`, {
          method: 'POST'
        });

        if (res.ok) {
          const data = await res.json().catch(() => ({}));
          setDeviceProgress(serial, 100, 'Token Erased');
          addLog(serial, 'INFO', data.message || `OK - AT+ENGMODES=0,0,3,0 success. Token erased offline.`);
          showToast(`Token offline berhasil dicabut dari ${serial}`, 'info');
        } else {
          const err = await res.json().catch(() => ({}));
          setDeviceProgress(serial, 100, 'Removal Error');
          addLog(serial, 'ERROR', `Gagal cabut token: ${err.message || 'AT command failed'}`);
          showToast(`Gagal cabut token di ${serial}: ${err.message || 'Error'}`, 'error');
        }
      } catch (e) {
        setDeviceProgress(serial, 100, 'Daemon Error');
        addLog(serial, 'ERROR', `Koneksi gagal ke daemon: ${e.message}`);
        console.error('Remove request failed:', e);
      }
    }

    setTimeout(fetchData, 800);
  }

  // --- Event Listeners ---
  function initEvents() {
    // Accordion Toggle
    if (accordionToggle && accordion) {
      accordionToggle.addEventListener('click', () => {
        accordion.classList.toggle('collapsed');
      });
    }

    // Terminal Drawer Controls
    if (btnOpenTerminal) {
      btnOpenTerminal.addEventListener('click', () => openTerminalDrawer(null));
    }
    if (btnCloseTerminal) {
      btnCloseTerminal.addEventListener('click', closeTerminalDrawer);
    }
    if (btnClearTerminal) {
      btnClearTerminal.addEventListener('click', () => {
        logs.length = 0;
        renderTerminalLogs();
      });
    }
    if (drawerBackdrop) {
      drawerBackdrop.addEventListener('click', (e) => {
        if (e.target === drawerBackdrop) closeTerminalDrawer();
      });
    }

    // Master Checkbox
    if (masterCheckbox) {
      masterCheckbox.addEventListener('change', (e) => {
        const filtered = getFilteredDevices();
        if (e.target.checked) {
          filtered.forEach(d => selectedSerials.add(d.serial));
        } else {
          filtered.forEach(d => selectedSerials.delete(d.serial));
        }
        updateSelectionRecap();
        renderFleetTable();
      });
    }

    // Dynamic Select All / Deselect All Toggle Button
    if (btnToggleSelectAll) {
      btnToggleSelectAll.addEventListener('click', (e) => {
        e.stopPropagation();
        const total = allDevices.length;
        if (selectedSerials.size === total && total > 0) {
          selectedSerials.clear();
        } else {
          allDevices.forEach(d => selectedSerials.add(d.serial));
        }
        updateSelectionRecap();
        renderFleetTable();
      });
    }

    // Batch Actions
    if (btnBatchInstall) {
      btnBatchInstall.addEventListener('click', () => {
        queryDevices(Array.from(selectedSerials));
      });
    }

    if (btnBatchRemove) {
      btnBatchRemove.addEventListener('click', () => {
        removeTokenOffline(Array.from(selectedSerials));
      });
    }

    // Filters
    if (inputSearch) {
      inputSearch.addEventListener('input', (e) => {
        filterSearch = e.target.value.trim();
        renderFleetTable();
      });
    }

    if (selectNode) {
      selectNode.addEventListener('change', (e) => {
        filterNode = e.target.value;
        renderFleetTable();
      });
    }

    // Tabs
    if (tabDevices) tabDevices.addEventListener('click', () => showView('devices'));
    if (tabNodes) tabNodes.addEventListener('click', () => showView('nodes'));
    if (navItemDevices) navItemDevices.addEventListener('click', () => showView('devices'));
    if (navItemNodes) navItemNodes.addEventListener('click', () => showView('nodes'));

    if (refreshBtn) {
      refreshBtn.addEventListener('click', () => {
        addLog('HUB', 'INFO', 'Manual fleet refresh requested');
        fetchData();
      });
    }

    // Keyboard navigation
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        closeTerminalDrawer();
      }
    });
  }

  // Initial Seed Logs
  addLog('HUB', 'INFO', 'MRT Central Fleet Station v2.0 initialized');
  addLog('HUB', 'INFO', 'Listening for USB device insertion on /dev/ttyACM*');

  // Init
  initTheme();
  initEvents();
  fetchData();

  // Polling every 5 seconds
  setInterval(fetchData, 5000);
})();
