(() => {
  const syncState = document.getElementById("sync-state");
  const refreshBtn = document.getElementById("refresh-now");
  const filterInput = document.getElementById("users-filter");
  const usersBody = document.getElementById("users-live-body");
  const warningsList = document.getElementById("warnings-live");
  const createUserForm = document.getElementById("create-user-form");
  const createUserAddress = document.getElementById("create-user-address");
  const createUserPassword = document.getElementById("create-user-password");
  const createUserSubmit = document.getElementById("create-user-submit");

  const metricUsers = document.getElementById("metric-users");
  const metricBans = document.getElementById("metric-bans");
  const metricQueue = document.getElementById("metric-queue");

  if (!syncState || !refreshBtn || !filterInput || !usersBody || !warningsList) return;

  let inFlight = false;
  let usersQuery = "";
  let dashboardTimer = null;
  let usersTimer = null;
  const csrfToken = window.__cmCsrfToken || "";

  function stampNow() {
    return new Date().toLocaleTimeString();
  }

  function renderUsers(items) {
    if (!items.length) {
      usersBody.innerHTML = `
        <tr class="border-b border-white/5 text-slate-300">
          <td class="px-3 py-4" colspan="6">No users matched the current filter.</td>
        </tr>`;
      return;
    }
    usersBody.innerHTML = items
      .map((item) => {
        const statusClass = item.blocked
          ? "bg-rose-400/20 text-rose-200"
          : "bg-emerald-400/20 text-emerald-200";
        const statusText = item.blocked ? "blocked" : "active";
        const actionLabel = item.blocked ? "Unblock" : "Block";
        const actionName = item.blocked ? "unblock" : "block";
        return `
          <tr class="border-b border-white/5 text-slate-200 animate-fade-in-fast">
            <td class="px-3 py-2">${item.address}</td>
            <td class="px-3 py-2"><span class="inline-flex rounded-full px-2 py-1 text-xs font-medium ${statusClass}">${statusText}</span></td>
            <td class="px-3 py-2">${item.mailbox_size || "unavailable"}</td>
            <td class="px-3 py-2">${item.message_count || "unavailable"}</td>
            <td class="px-3 py-2">${item.last_seen || "unavailable"}</td>
            <td class="px-3 py-2">
              <div class="flex flex-wrap gap-2">
                <button class="btn btn-soft" type="button" data-action="${actionName}" data-address="${item.address}">${actionLabel}</button>
                <button class="btn btn-danger" type="button" data-action="delete" data-address="${item.address}">Delete</button>
              </div>
            </td>
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

  async function apiAction(url, payload) {
    const response = await fetch(url, {
      method: "POST",
      credentials: "same-origin",
      headers: {
        Accept: "application/json",
        "Content-Type": "application/json",
        "X-CSRF-Token": csrfToken,
      },
      body: JSON.stringify(payload),
    });
    if (!response.ok) {
      const text = await response.text();
      throw new Error(text || `${url} failed: ${response.status}`);
    }
    return response.json();
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
  usersBody.addEventListener("click", async (event) => {
    const button = event.target.closest("button[data-action][data-address]");
    if (!button) return;
    const action = button.dataset.action;
    const address = button.dataset.address;
    if (!action || !address) return;

    const row = button.closest("tr");
    if (!row) return;

    button.disabled = true;
    row.style.opacity = "0.7";

    try {
      if (action === "block") {
        const statusBadge = row.querySelector("span");
        if (statusBadge) {
          statusBadge.textContent = "blocked";
          statusBadge.className =
            "inline-flex rounded-full px-2 py-1 text-xs font-medium bg-rose-400/20 text-rose-200";
        }
        button.dataset.action = "unblock";
        button.textContent = "Unblock";
        await apiAction("/api/v1/admin/users/block", { address });
      } else if (action === "unblock") {
        const statusBadge = row.querySelector("span");
        if (statusBadge) {
          statusBadge.textContent = "active";
          statusBadge.className =
            "inline-flex rounded-full px-2 py-1 text-xs font-medium bg-emerald-400/20 text-emerald-200";
        }
        button.dataset.action = "block";
        button.textContent = "Block";
        await apiAction("/api/v1/admin/users/unblock", { address });
      } else if (action === "delete") {
        if (!window.confirm(`Delete ${address}?`)) {
          row.style.opacity = "";
          button.disabled = false;
          return;
        }
        row.remove();
        await apiAction("/api/v1/admin/users/delete-account", { address });
      }
      syncState.textContent = `Action applied at ${stampNow()}`;
    } catch (error) {
      console.error(error);
      syncState.textContent = `Action failed at ${stampNow()}`;
      await refreshAll();
    } finally {
      row.style.opacity = "";
      button.disabled = false;
    }
  });

  if (createUserForm && createUserAddress && createUserPassword && createUserSubmit) {
    createUserForm.addEventListener("submit", async (event) => {
      event.preventDefault();
      const address = createUserAddress.value.trim().toLowerCase();
      const password = createUserPassword.value;
      if (!address || !password) return;

      createUserSubmit.disabled = true;
      syncState.textContent = "Creating user...";
      try {
        await apiAction("/api/v1/admin/users/create", { address, password });
        createUserForm.reset();
        await refreshAll();
        syncState.textContent = `User created at ${stampNow()}`;
      } catch (error) {
        console.error(error);
        syncState.textContent = `Create failed at ${stampNow()}`;
      } finally {
        createUserSubmit.disabled = false;
      }
    });
  }

  schedule();
  refreshAll().catch((error) => console.error(error));
})();
