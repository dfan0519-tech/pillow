/**
 * Wave C2-E — project resource isolation and recents UI helper.
 *
 * Public DOM + portable relative paths only. Never touches filesystem, Tauri
 * invoke, SQLite, or machine-root text. Does not create fixtures, launch or
 * close processes, or select a second project on its own.
 *
 * Returns JSON-serializable evidence objects only.
 */

import assert from "node:assert/strict";

/**
 * Renderer search debounce is 250ms (`resourcePaneState.RESOURCE_SEARCH_DEBOUNCE_MS`).
 * Helpers wait on public search-result DOM rather than sleeping that constant.
 */
const DEFAULT_TIMEOUT_MS = 20_000;

/**
 * @param {string} value
 * @param {string} label
 */
function assertPortableRelativePath(value, label) {
  assert.equal(typeof value, "string", `${label} must be a string`);
  const path = value.trim();
  assert.ok(path.length > 0, `${label} must be non-empty`);
  assert.ok(!path.includes("\\"), `${label} must use portable separators`);
  assert.ok(!path.startsWith("/"), `${label} must be project-relative`);
  assert.ok(!/^[A-Za-z]:/u.test(path), `${label} must not be a drive path`);
  assert.ok(!path.includes("://"), `${label} must not be a URL`);
  assert.ok(
    !path.split("/").some((part) => part === ".." || part === ""),
    `${label} must not contain empty or parent segments`,
  );
}

/**
 * @param {string} value
 * @param {string} label
 */
function assertNonEmptyString(value, label) {
  assert.equal(typeof value, "string", `${label} must be a string`);
  assert.ok(value.trim().length > 0, `${label} must be non-empty`);
}

/**
 * @param {import('playwright-core').Page} page
 */
function resourcePane(page) {
  return page.getByTestId("resource-pane");
}

/**
 * @param {string} text
 * @param {string} needle
 * @param {string} message
 */
function assertIncludes(text, needle, message) {
  assert.ok(text.includes(needle), message);
}

/**
 * @param {string} text
 * @param {string} needle
 * @param {string} message
 */
function assertExcludes(text, needle, message) {
  assert.ok(!text.includes(needle), message);
}

/**
 * @param {import('playwright-core').Page} page
 * @param {string} foreignPath
 * @param {string} foreignMarker
 */
async function assertForeignAbsentFromResourceContext(
  page,
  foreignPath,
  foreignMarker,
) {
  const pane = resourcePane(page);
  await pane.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  const text = await pane.innerText();
  assertExcludes(
    text,
    foreignPath,
    `foreign path must be absent from resource context: ${foreignPath}`,
  );
  assertExcludes(
    text,
    foreignMarker,
    `foreign marker must be absent from resource context: ${foreignMarker}`,
  );
}

/**
 * @param {import('playwright-core').Page} page
 */
async function waitForBrowseReady(page) {
  const pane = resourcePane(page);
  await pane.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  await page.getByRole("region", { name: "项目资源" }).waitFor({
    state: "visible",
    timeout: DEFAULT_TIMEOUT_MS,
  });

  await page.waitForFunction(
    () => {
      const root = document.querySelector('[data-testid="resource-pane"]');
      if (!root) return false;
      const body = root.textContent ?? "";
      // Browse mode shows recents and/or files once ready; bare loading is not ready.
      if (
        body.includes("加载中…")
        && !body.includes("暂无最近打开")
        && !body.includes("暂无文件")
        && !root.querySelector('[aria-label="最近打开"] button')
        && !root.querySelector('[aria-label="项目文件"] button')
      ) {
        return false;
      }
      const hasRecents = Boolean(root.querySelector('[aria-label="最近打开"]'))
        || body.includes("暂无最近打开");
      const hasFiles = Boolean(root.querySelector('[aria-label="项目文件"]'))
        || body.includes("暂无文件");
      return hasRecents || hasFiles;
    },
    { timeout: DEFAULT_TIMEOUT_MS },
  );
}

/**
 * Close the contextual pane if open, then open it via keyboard focus + activation.
 * Proves focus and expanded state through public controls only.
 *
 * @param {import('playwright-core').Page} page
 */
async function openResourcePaneByKeyboard(page) {
  const toggle = page.getByTestId("resource-toggle");
  await toggle.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  assert.equal(
    await toggle.isDisabled(),
    false,
    "resource-toggle must be enabled for an active project",
  );

  if ((await toggle.getAttribute("aria-pressed")) === "true") {
    await toggle.focus();
    await page.keyboard.press("Enter");
    await resourcePane(page).waitFor({
      state: "detached",
      timeout: DEFAULT_TIMEOUT_MS,
    });
  }

  await toggle.focus();
  const focused = await toggle.evaluate((el) => el === document.activeElement);
  assert.equal(focused, true, "resource-toggle must receive keyboard focus");

  await page.keyboard.press("Enter");

  const pane = resourcePane(page);
  await pane.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  assert.equal(
    await toggle.getAttribute("aria-pressed"),
    "true",
    "resource-toggle must report expanded (aria-pressed=true)",
  );
  await page.getByLabel("上下文资源").waitFor({
    state: "visible",
    timeout: DEFAULT_TIMEOUT_MS,
  });
  await page.getByRole("region", { name: "项目资源" }).waitFor({
    state: "visible",
    timeout: DEFAULT_TIMEOUT_MS,
  });

  return {
    keyboardFocused: true,
    paneExpanded: true,
    toggleLabel: await toggle.getAttribute("aria-label"),
  };
}

/**
 * Expand directory segments and open a portable file path from the project tree.
 *
 * @param {import('playwright-core').Page} page
 * @param {string} relativePath
 */
async function openPortablePathViaTree(page, relativePath) {
  const pane = resourcePane(page);
  const filesRegion = pane.getByRole("region", { name: "项目文件" });
  await filesRegion.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });

  const parts = relativePath.split("/");
  for (let index = 0; index < parts.length - 1; index += 1) {
    const segment = parts[index];
    const dirButton = filesRegion.getByRole("button", {
      name: segment,
      exact: true,
    });
    await dirButton.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
    if ((await dirButton.getAttribute("aria-expanded")) !== "true") {
      await dirButton.click();
    }
    await page.waitForFunction(
      (label) => {
        const root = document.querySelector('[data-testid="resource-pane"]');
        if (!root) return false;
        const buttons = [...root.querySelectorAll("button")];
        const dir = buttons.find((btn) => btn.getAttribute("aria-label") === label);
        if (!dir || dir.getAttribute("aria-expanded") !== "true") return false;
        const item = dir.closest("li");
        const status = item?.textContent ?? "";
        return !status.includes("加载中…");
      },
      segment,
      { timeout: DEFAULT_TIMEOUT_MS },
    );
  }

  const leaf = parts[parts.length - 1];
  const nestedFile = filesRegion.getByRole("button", {
    name: relativePath,
    exact: true,
  });
  if ((await nestedFile.count()) > 0) {
    await nestedFile.first().click();
    return;
  }

  await filesRegion
    .getByRole("button", { name: leaf, exact: true })
    .first()
    .click();
}

/**
 * @param {import('playwright-core').Page} page
 * @param {{ path: string, marker: string }} expected
 */
async function waitForOwnPreview(page, expected) {
  const preview = page.getByTestId("resource-preview");
  await preview.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  await page.waitForFunction(
    ({ path, marker }) => {
      const el = document.querySelector('[data-testid="resource-preview"]');
      if (!el) return false;
      const text = el.textContent ?? "";
      return text.includes(path) && text.includes(marker);
    },
    { path: expected.path, marker: expected.marker },
    { timeout: DEFAULT_TIMEOUT_MS },
  );
  const text = await preview.innerText();
  assertIncludes(text, expected.path, `preview must show own path ${expected.path}`);
  assertIncludes(
    text,
    expected.marker,
    `preview must show own marker ${expected.marker}`,
  );
  return {
    previewVisible: true,
    previewOwnPath: true,
    previewOwnMarker: true,
  };
}

/**
 * Clear the resource search input and wait for browse (recents/tree) mode.
 *
 * @param {import('playwright-core').Page} page
 */
async function clearSearchAndShowBrowse(page) {
  const pane = resourcePane(page);
  const search = pane.getByLabel("搜索项目资源");
  await search.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  await search.fill("");
  // Debounced query settles back to empty → browse mode restores recents/tree.
  await page.waitForFunction(
    () => {
      const root = document.querySelector('[data-testid="resource-pane"]');
      if (!root) return false;
      const searchRegion = root.querySelector('[aria-label="搜索结果"]');
      if (searchRegion) return false;
      return Boolean(root.querySelector('[aria-label="最近打开"]'));
    },
    { timeout: DEFAULT_TIMEOUT_MS },
  );
  await waitForBrowseReady(page);
  await pane.getByRole("region", { name: "最近打开" }).waitFor({
    state: "visible",
    timeout: DEFAULT_TIMEOUT_MS,
  });
}

/**
 * Select a project by its public sidebar accessible name and wait for the
 * winning active project heading.
 *
 * @param {import('playwright-core').Page} page
 * @param {string} projectName
 */
export async function selectProject(page, projectName) {
  assertNonEmptyString(projectName, "projectName");

  const sidebar = page.getByTestId("app-sidebar");
  await sidebar.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });

  const projectButton = sidebar.getByRole("button", {
    name: projectName,
    exact: true,
  });
  await projectButton.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });
  await projectButton.click();

  await page.waitForFunction(
    (name) => {
      const sidebarEl = document.querySelector('[data-testid="app-sidebar"]');
      if (!sidebarEl) return false;
      const buttons = [...sidebarEl.querySelectorAll("button[aria-label]")];
      const match = buttons.find(
        (btn) => btn.getAttribute("aria-label") === name,
      );
      if (!match || match.getAttribute("aria-pressed") !== "true") return false;
      const heading = document.querySelector(
        '[data-testid="active-project-name"]',
      );
      return Boolean(heading?.textContent?.includes(name));
    },
    projectName,
    { timeout: DEFAULT_TIMEOUT_MS },
  );

  const activeHeading = page.getByTestId("active-project-name");
  const activeText = (await activeHeading.innerText()).trim();
  assertIncludes(
    activeText,
    projectName,
    `active project heading must show ${projectName}`,
  );
  assert.equal(
    await projectButton.getAttribute("aria-pressed"),
    "true",
    `sidebar project ${projectName} must be aria-pressed`,
  );

  return {
    selectedProjectName: projectName,
    activeProjectName: activeText,
    sidebarPressed: true,
  };
}

/**
 * Exercise tree, debounced search, bounded Markdown preview, and recents for
 * the already-selected project. Own path/marker must appear; foreign must not.
 *
 * @param {{
 *   page: import('playwright-core').Page,
 *   ownPath: string,
 *   ownMarker: string,
 *   foreignPath: string,
 *   foreignMarker: string,
 * }} args
 */
export async function exerciseProjectResources({
  page,
  ownPath,
  ownMarker,
  foreignPath,
  foreignMarker,
}) {
  assertPortableRelativePath(ownPath, "ownPath");
  assertPortableRelativePath(foreignPath, "foreignPath");
  assertNonEmptyString(ownMarker, "ownMarker");
  assertNonEmptyString(foreignMarker, "foreignMarker");
  assert.notEqual(ownPath, foreignPath, "own and foreign paths must differ");
  assert.notEqual(ownMarker, foreignMarker, "own and foreign markers must differ");

  async function step(name, work) {
    try {
      return await work();
    } catch (error) {
      const detail = error instanceof Error ? error.message : String(error);
      throw new Error(`exerciseProjectResources:${name}: ${detail}`);
    }
  }

  const keyboard = await step("open-pane-keyboard", () =>
    openResourcePaneByKeyboard(page),
  );
  await step("browse-ready", () => waitForBrowseReady(page));
  await step("foreign-absent-initial", () =>
    assertForeignAbsentFromResourceContext(page, foreignPath, foreignMarker),
  );

  // --- Root tree → bounded preview ---
  await step("tree-open", () => openPortablePathViaTree(page, ownPath));
  const treePreview = await step("tree-preview", () =>
    waitForOwnPreview(page, {
      path: ownPath,
      marker: ownMarker,
    }),
  );
  await step("foreign-absent-after-tree", () =>
    assertForeignAbsentFromResourceContext(page, foreignPath, foreignMarker),
  );

  const paneAfterTree = await resourcePane(page).innerText();
  assertIncludes(
    paneAfterTree,
    ownPath,
    `tree/resource context must show own path ${ownPath}`,
  );

  // --- Debounced search ---
  // Public contract (ResourcePane search hit):
  //   <button aria-label={`${hit.path} 第 ${hit.line} 行`}> with visible path text.
  // Kernel search is content-based (line text), not path-name matching — query the
  // own marker that lives in the fixture file so a real hit is produced.
  // Do NOT wait on whole-pane textContent: after a tree open, the bounded preview
  // already contains ownPath and would false-positive before search hits render.
  const pane = resourcePane(page);
  const search = pane.getByLabel("搜索项目资源");
  await step("search-fill-marker", async () => {
    await search.click();
    await search.fill("");
    await search.fill(ownMarker);
  });

  const searchRegion = pane.getByRole("region", { name: "搜索结果" });
  await step("search-region-visible", () =>
    searchRegion.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS }),
  );

  const escapedOwnPath = ownPath.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  // aria-label is exactly `${path} 第 ${line} 行` (portable path + line meta).
  const searchHitAccessibleName = new RegExp(
    `^${escapedOwnPath} 第 \\d+ 行$`,
  );
  const searchHit = searchRegion.getByRole("button", {
    name: searchHitAccessibleName,
  });

  // Settle on the public hit button, not pane-wide path text (preview contamination).
  await step("search-hit-settle", () =>
    page.waitForFunction(
      (path) => {
        const root = document.querySelector('[data-testid="resource-pane"]');
        if (!root) return false;
        const region = root.querySelector('[aria-label="搜索结果"]');
        if (!region) return false;
        if ((region.textContent ?? "").includes("搜索中…")) return false;
        const buttons = [...region.querySelectorAll("button[aria-label]")];
        return buttons.some((btn) => {
          const label = btn.getAttribute("aria-label") ?? "";
          // Match ResourcePane contract: `${path} 第 ${line} 行`
          if (!label.startsWith(`${path} 第 `)) return false;
          return / 第 \d+ 行$/u.test(label);
        });
      },
      ownPath,
      { timeout: DEFAULT_TIMEOUT_MS },
    ),
  );

  await step("search-hit-visible", () =>
    searchHit.first().waitFor({
      state: "visible",
      timeout: DEFAULT_TIMEOUT_MS,
    }),
  );

  const searchText = await searchRegion.innerText();
  assertIncludes(searchText, ownPath, `search results must include ${ownPath}`);
  assertExcludes(
    searchText,
    foreignPath,
    `search results must not include foreign path ${foreignPath}`,
  );
  assertExcludes(
    searchText,
    foreignMarker,
    `search results must not include foreign marker ${foreignMarker}`,
  );

  await step("search-hit-click", () => searchHit.first().click());
  const searchPreview = await step("search-preview", () =>
    waitForOwnPreview(page, {
      path: ownPath,
      marker: ownMarker,
    }),
  );
  await step("foreign-absent-after-search", () =>
    assertForeignAbsentFromResourceContext(page, foreignPath, foreignMarker),
  );

  // --- Recents after open ---
  // preview_project_resource records durable recents in SQLite, but the browser
  // loads listRecentProjectResources only on mount/open/project change — not when
  // leaving search mode. Collapse + expand the host pane (public keyboard path)
  // remounts the browser so recents reflect the opens we just performed.
  await step("remount-browse-for-recents", async () => {
    await openResourcePaneByKeyboard(page);
    await waitForBrowseReady(page);
  });
  await step("foreign-absent-after-remount", () =>
    assertForeignAbsentFromResourceContext(page, foreignPath, foreignMarker),
  );

  const recentsRegion = resourcePane(page).getByRole("region", {
    name: "最近打开",
  });
  await step("recents-region", () =>
    recentsRegion.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS }),
  );

  const ownRecent = recentsRegion.getByRole("button", {
    name: ownPath,
    exact: true,
  });
  await step("recents-own-row", () =>
    ownRecent.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS }),
  );

  const recentsText = await recentsRegion.innerText();
  assertIncludes(recentsText, ownPath, `recents must include ${ownPath}`);
  assertExcludes(
    recentsText,
    foreignPath,
    `recents must not include foreign path ${foreignPath}`,
  );
  assertExcludes(
    recentsText,
    foreignMarker,
    `recents must not include foreign marker ${foreignMarker}`,
  );

  await step("recents-click", () => ownRecent.click());
  const recentsPreview = await step("recents-preview", () =>
    waitForOwnPreview(page, {
      path: ownPath,
      marker: ownMarker,
    }),
  );
  await step("foreign-absent-after-recents", () =>
    assertForeignAbsentFromResourceContext(page, foreignPath, foreignMarker),
  );

  return {
    keyboardFocused: keyboard.keyboardFocused,
    paneExpanded: keyboard.paneExpanded,
    toggleLabel: keyboard.toggleLabel,
    treeOwnPathVisible: true,
    treePreview,
    searchOwnPathVisible: true,
    searchForeignAbsent: true,
    searchPreview,
    searchClearedBeforeRecents: true,
    browseRemountedForRecents: true,
    recentsOwnPathVisible: true,
    recentsForeignAbsent: true,
    recentsPreview,
    resourceContextForeignAbsent: true,
    ownPath,
    foreignPathAbsent: foreignPath,
  };
}

/**
 * After restart (or when recents should already be durable), open the own
 * recent row and prove the own bounded preview while foreign data stays absent.
 * Assumes the target project is already selected by the orchestrator.
 *
 * @param {{
 *   page: import('playwright-core').Page,
 *   ownPath: string,
 *   ownMarker: string,
 *   foreignPath: string,
 *   foreignMarker: string,
 * }} args
 */
export async function assertRestoredProjectRecents({
  page,
  ownPath,
  ownMarker,
  foreignPath,
  foreignMarker,
}) {
  assertPortableRelativePath(ownPath, "ownPath");
  assertPortableRelativePath(foreignPath, "foreignPath");
  assertNonEmptyString(ownMarker, "ownMarker");
  assertNonEmptyString(foreignMarker, "foreignMarker");

  const keyboard = await openResourcePaneByKeyboard(page);
  await waitForBrowseReady(page);

  // Ensure search is empty so recents (not search hits) are inspected.
  const pane = resourcePane(page);
  const search = pane.getByLabel("搜索项目资源");
  if ((await search.inputValue()).trim().length > 0) {
    await clearSearchAndShowBrowse(page);
  } else {
    await pane.getByRole("region", { name: "最近打开" }).waitFor({
      state: "visible",
      timeout: DEFAULT_TIMEOUT_MS,
    });
  }

  await assertForeignAbsentFromResourceContext(page, foreignPath, foreignMarker);

  const recentsRegion = pane.getByRole("region", { name: "最近打开" });
  const ownRecent = recentsRegion.getByRole("button", {
    name: ownPath,
    exact: true,
  });
  await ownRecent.waitFor({ state: "visible", timeout: DEFAULT_TIMEOUT_MS });

  const foreignRecent = recentsRegion.getByRole("button", {
    name: foreignPath,
    exact: true,
  });
  assert.equal(
    await foreignRecent.count(),
    0,
    `restored recents must not list foreign path ${foreignPath}`,
  );

  const recentsText = await recentsRegion.innerText();
  assertIncludes(recentsText, ownPath, `restored recents must include ${ownPath}`);
  assertExcludes(
    recentsText,
    foreignPath,
    `restored recents must not include ${foreignPath}`,
  );
  assertExcludes(
    recentsText,
    foreignMarker,
    `restored recents must not include foreign marker ${foreignMarker}`,
  );

  await ownRecent.click();
  const preview = await waitForOwnPreview(page, {
    path: ownPath,
    marker: ownMarker,
  });
  await assertForeignAbsentFromResourceContext(page, foreignPath, foreignMarker);

  return {
    keyboardFocused: keyboard.keyboardFocused,
    paneExpanded: keyboard.paneExpanded,
    restoredRecentsOwnPathVisible: true,
    restoredRecentsForeignAbsent: true,
    restoredPreview: preview,
    resourceContextForeignAbsent: true,
    ownPath,
  };
}
