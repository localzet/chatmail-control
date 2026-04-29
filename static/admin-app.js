(() => {
  const syncState = document.getElementById("sync-state");
  const refreshBtn = document.getElementById("refresh-now");
  const filterInput = document.getElementById("users-filter");
  const usersBody = document.getElementById("users-live-body");
  const warningsList = document.getElementById("warnings-live");

  const metricUsers = document.getElementById("metric-users");
  const metricBans = document.getElementById("metric-bans");
  const metricQueue = document.getElementById("metric-queue");

  if (!syncState || !refreshBtn || !filterInput || !usersBody || !warningsList) return;

  let inFlight = false;
  let usersQuery = "";
  let dashboardTimer = null;
  let usersTimer = null;

  function stampNow() {
    return new Date().toLocaleTimeString();
  }

  function renderUsers(items) {
    if (!items.length) {
      usersBody.innerHTML = `
        <tr class="border-b border-white/5 text-slate-300">
          <td class="px-3 py-4" colspan="5">No users matched the current filter.</td>
        </tr>`;
      return;
    }
    usersBody.innerHTML = items
      .map((item) => {
        const statusClass = item.blocked
          ? "bg-rose-400/20 text-rose-200"
          : "bg-emerald-400/20 text-emerald-200";
        const statusText = item.blocked ? "blocked" : "active";
        return `
          <tr class="border-b border-white/5 text-slate-200 animate-fade-in-fast">
            <td class="px-3 py-2">${item.address}</td>
            <td class="px-3 py-2"><span class="inline-flex rounded-full px-2 py-1 text-xs font-medium ${statusClass}">${statusText}</span></td>
            <td class="px-3 py-2">${item.mailbox_size || "unavailable"}</td>
            <td class="px-3 py-2">${item.message_count || "unavailable"}</td>
            <td class="px-3 py-2">${item.last_seen || "unavailable"}</td>
          </tr>`;
      })
      .join("");
  }

  function renderWarnings(warnings) {
    if (!warnings.length) {
      warningsList.innerHTML = `<li class="text-sm text-slate-400">No warnings</li>`;
      return;
    }
    warningsList.innerHTML = warnings
      .map(
        (item) => `
      <li class="rounded-xl border border-amber-400/30 bg-amber-400/10 px-3 py-2 text-sm text-amber-100 animate-fade-in-fast">
        <strong>${item.name}</strong>: ${item.details}
      </li>`
      )
      .join("");
  }

  async function loadDashboard() {
    const response = await fetch("/api/v1/admin/dashboard", {
      headers: { Accept: "application/json" },
      credentials: "same-origin",
    });
    if (!response.ok) throw new Error(`dashboard failed: ${response.status}`);
    const data = await response.json();
    metricUsers.textContent = String(data.users_count);
    metricBans.textContent = String(data.active_bans_count);
    metricQueue.textContent = String(data.mail_queue_size);
    renderWarnings(data.warnings || []);
  }

  async function loadUsers() {
    const params = new URLSearchParams();
    params.set("limit", "250");
    if (usersQuery) params.set("q", usersQuery);
    const response = await fetch(`/api/v1/admin/users?${params.toString()}`, {
      headers: { Accept: "application/json" },
      credentials: "same-origin",
    });
    if (!response.ok) throw new Error(`users failed: ${response.status}`);
    const data = await response.json();
    renderUsers(data.users || []);
  }

  async function refreshAll() {
    if (inFlight) return;
    inFlight = true;
    syncState.textContent = "Syncing...";
    try {
      await Promise.all([loadDashboard(), loadUsers()]);
      syncState.textContent = `Synced at ${stampNow()}`;
    } catch (error) {
      syncState.textContent = `Sync failed at ${stampNow()}`;
      console.error(error);
    } finally {
      inFlight = false;
    }
  }

  function schedule() {
    clearInterval(dashboardTimer);
    clearInterval(usersTimer);
    dashboardTimer = setInterval(refreshAll, 15000);
    usersTimer = setInterval(loadUsers, 5000);
  }

  const onFilterInput = (() => {
    let timer = null;
    return () => {
      clearTimeout(timer);
      timer = setTimeout(() => {
        usersQuery = filterInput.value.trim().toLowerCase();
        loadUsers().catch((error) => console.error(error));
      }, 250);
    };
  })();

  filterInput.addEventListener("input", onFilterInput);
  refreshBtn.addEventListener("click", () => {
    refreshAll().catch((error) => console.error(error));
  });

  schedule();
  refreshAll().catch((error) => console.error(error));
})();
