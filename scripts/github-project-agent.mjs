#!/usr/bin/env node
import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";

const githubApi = "https://api.github.com";
const graphqlApi = "https://api.github.com/graphql";
const runtimeDir = ".github/codex/runtime";

const config = {
  owner: env("AGENT_PROJECT_OWNER"),
  ownerType: process.env.AGENT_PROJECT_OWNER_TYPE || "auto",
  number: numberEnv("AGENT_PROJECT_NUMBER"),
  statusField: process.env.AGENT_PROJECT_STATUS_FIELD || "Status",
  statuses: {
    inbox: process.env.AGENT_PROJECT_STATUS_INBOX || "Inbox",
    clarifying: process.env.AGENT_PROJECT_STATUS_CLARIFYING || "Clarifying",
    ready: process.env.AGENT_PROJECT_STATUS_READY || "Ready for Agent",
    inProgress: process.env.AGENT_PROJECT_STATUS_IN_PROGRESS || "In Progress",
    prOpen: process.env.AGENT_PROJECT_STATUS_PR_OPEN || "PR Open",
    blocked: process.env.AGENT_PROJECT_STATUS_BLOCKED || "Blocked",
  },
};

const command = process.argv[2];

try {
  switch (command) {
    case "prepare-intake":
      await prepareIntake();
      break;
    case "apply-intake":
      await applyIntake(requiredArg(3, "result path"));
      break;
    case "prepare-worker":
      await prepareWorker();
      break;
    case "apply-worker":
      await applyWorker(requiredArg(3, "result path"));
      break;
    case "finalize-pr":
      await finalizePr(requiredArg(3, "result path"));
      break;
    case "fail-no-changes":
      await failNoChanges(requiredArg(3, "result path"));
      break;
    case "fail-run":
      await failRun();
      break;
    default:
      throw new Error(`Unknown command: ${command || "(missing)"}`);
  }
} catch (error) {
  console.error(error instanceof Error ? error.stack : error);
  process.exit(1);
}

async function prepareIntake() {
  const event = readEvent();
  const issue = event.issue;
  if (!issue || issue.pull_request || issue.state === "closed" || isBot(event.sender)) {
    setOutput("run_codex", "false");
    return;
  }

  const project = await loadProject();
  const item = await ensureIssueProjectItem(project, issue);
  await setStatus(project, item.id, config.statuses.clarifying);
  await writeIssueContext(issue, "Issue intake");

  setOutput("run_codex", "true");
  setOutput("issue_number", String(issue.number));
  setOutput("issue_url", issue.html_url);
  setOutput("project_item_id", item.id);
}

async function applyIntake(resultPath) {
  const result = readJson(resultPath);
  const decision = requiredEnum(result.decision, [
    "needs_clarification",
    "ready",
    "blocked",
  ]);
  const project = await loadProject();
  const issueNumber = numberEnv("ISSUE_NUMBER");
  const itemId = env("PROJECT_ITEM_ID");

  if (decision === "needs_clarification") {
    await postIssueComment(
      issueNumber,
      result.comment || "What should be clarified before work starts?",
      "intake",
    );
    await setStatus(project, itemId, config.statuses.clarifying);
  } else if (decision === "ready") {
    await setStatus(project, itemId, config.statuses.ready);
  } else {
    await postIssueComment(
      issueNumber,
      result.comment || "I cannot safely turn this into agent work yet.",
      "intake-blocked",
    );
    await setStatus(project, itemId, config.statuses.blocked);
  }

  setOutput("decision", decision);
}

async function prepareWorker() {
  const project = await loadProject();
  const items = await listProjectItems(project.id);
  const item = items.find((candidate) => {
    const content = candidate.content;
    return (
      content?.type === "Issue" &&
      content.repository === process.env.GITHUB_REPOSITORY &&
      content.state === "OPEN" &&
      candidate.status === config.statuses.ready
    );
  });

  if (!item) {
    setOutput("selected", "false");
    return;
  }

  await setStatus(project, item.id, config.statuses.inProgress);
  await writeIssueContext(item.content, "Scheduled agent worker");

  setOutput("selected", "true");
  setOutput("issue_number", String(item.content.number));
  setOutput("issue_url", item.content.url);
  setOutput("project_item_id", item.id);
  setOutput("branch", branchName(item.content.number, item.content.title));
}

async function applyWorker(resultPath) {
  const result = readJson(resultPath);
  const outcome = requiredEnum(result.outcome, [
    "completed",
    "needs_clarification",
    "blocked",
  ]);
  const issueNumber = numberEnv("ISSUE_NUMBER");
  const itemId = env("PROJECT_ITEM_ID");
  const project = await loadProject();

  if (outcome === "needs_clarification") {
    await postIssueComment(
      issueNumber,
      result.comment || "I need one more clarification before I can continue.",
      "worker-clarification",
    );
    await setStatus(project, itemId, config.statuses.clarifying);
  } else if (outcome === "blocked") {
    await postIssueComment(
      issueNumber,
      result.comment || result.summary || "The agent run is blocked.",
      "worker-blocked",
    );
    await setStatus(project, itemId, config.statuses.blocked);
  } else {
    writeFileSync(`${runtimeDir}/pr-body.md`, renderPullRequestBody(result));
    setOutput("pr_title", result.pr_title || `Work issue #${issueNumber}`);
  }

  setOutput("outcome", outcome);
}

async function finalizePr(resultPath) {
  const result = readJson(resultPath);
  const issueNumber = numberEnv("ISSUE_NUMBER");
  const itemId = env("PROJECT_ITEM_ID");
  const prUrl = env("PR_URL");
  const project = await loadProject();

  await setStatus(project, itemId, config.statuses.prOpen);
  await postIssueComment(
    issueNumber,
    [
      `Opened draft PR: ${prUrl}`,
      "",
      result.summary ? `Summary: ${result.summary}` : "",
      renderTests(result.tests),
    ]
      .filter(Boolean)
      .join("\n"),
    "pr-open",
  );
}

async function failNoChanges(resultPath) {
  const result = readJson(resultPath);
  const issueNumber = numberEnv("ISSUE_NUMBER");
  const itemId = env("PROJECT_ITEM_ID");
  const project = await loadProject();

  await postIssueComment(
    issueNumber,
    [
      "The agent reported completion, but the workflow found no file changes to commit.",
      "",
      result.summary ? `Reported summary: ${result.summary}` : "",
    ]
      .filter(Boolean)
      .join("\n"),
    "no-changes",
  );
  await setStatus(project, itemId, config.statuses.blocked);
}

async function failRun() {
  const issueNumber = numberEnv("ISSUE_NUMBER");
  const itemId = env("PROJECT_ITEM_ID");
  const project = await loadProject();

  await postIssueComment(
    issueNumber,
    [
      "The scheduled agent workflow failed before it could finish this issue.",
      "",
      `Run: ${process.env.GITHUB_SERVER_URL}/${process.env.GITHUB_REPOSITORY}/actions/runs/${process.env.GITHUB_RUN_ID}`,
    ].join("\n"),
    "worker-run-failed",
  );
  await setStatus(project, itemId, config.statuses.blocked);
}

async function loadProject() {
  const projectToken = env("PROJECT_TOKEN");
  const project = await findProject(projectToken);
  const statusField = project.fields.find(
    (field) => field.name === config.statusField && field.options,
  );
  if (!statusField) {
    throw new Error(
      `Project field "${config.statusField}" must be a single-select field`,
    );
  }

  return { ...project, statusField };
}

async function findProject(token) {
  const ownerTypes =
    config.ownerType === "auto"
      ? ["organization", "user"]
      : [config.ownerType.toLowerCase()];

  for (const ownerType of ownerTypes) {
    const queryRoot = ownerType === "user" ? "user" : "organization";
    const data = await graphql(
      token,
      `
        query($owner: String!, $number: Int!) {
          ${queryRoot}(login: $owner) {
            projectV2(number: $number) {
              id
              title
              fields(first: 100) {
                nodes {
                  ... on ProjectV2Field { id name }
                  ... on ProjectV2IterationField { id name }
                  ... on ProjectV2SingleSelectField {
                    id
                    name
                    options { id name }
                  }
                }
              }
            }
          }
        }
      `,
      { owner: config.owner, number: config.number },
    );
    const project = data[queryRoot]?.projectV2;
    if (project) {
      return {
        id: project.id,
        title: project.title,
        fields: project.fields.nodes.filter(Boolean),
      };
    }
  }

  throw new Error(
    `Project ${config.owner}/${config.number} was not found as an organization or user project`,
  );
}

async function ensureIssueProjectItem(project, issue) {
  const existing = await findProjectItemByContent(project.id, issue.node_id);
  if (existing) {
    return existing;
  }

  const data = await graphql(
    env("PROJECT_TOKEN"),
    `
      mutation($project: ID!, $content: ID!) {
        addProjectV2ItemById(input: { projectId: $project, contentId: $content }) {
          item { id }
        }
      }
    `,
    { project: project.id, content: issue.node_id },
  );
  return data.addProjectV2ItemById.item;
}

async function findProjectItemByContent(projectId, contentId) {
  const items = await listProjectItems(projectId);
  return items.find((item) => item.content?.id === contentId);
}

async function listProjectItems(projectId) {
  const items = [];
  let cursor = null;
  do {
    const data = await graphql(
      env("PROJECT_TOKEN"),
      `
        query($project: ID!, $cursor: String) {
          node(id: $project) {
            ... on ProjectV2 {
              items(first: 100, after: $cursor) {
                pageInfo { hasNextPage endCursor }
                nodes {
                  id
                  fieldValues(first: 30) {
                    nodes {
                      ... on ProjectV2ItemFieldSingleSelectValue {
                        name
                        field { ... on ProjectV2SingleSelectField { name } }
                      }
                    }
                  }
                  content {
                    ... on Issue {
                      __typename
                      id
                      number
                      title
                      body
                      state
                      url
                      repository { nameWithOwner }
                    }
                    ... on PullRequest {
                      __typename
                      id
                      number
                      title
                      state
                      url
                      repository { nameWithOwner }
                    }
                  }
                }
              }
            }
          }
        }
      `,
      { project: projectId, cursor },
    );

    const page = data.node.items;
    for (const node of page.nodes.filter(Boolean)) {
      const content = normalizeContent(node.content);
      items.push({
        id: node.id,
        content,
        status: statusValue(node, config.statusField),
      });
    }
    cursor = page.pageInfo.hasNextPage ? page.pageInfo.endCursor : null;
  } while (cursor);

  return items;
}

async function setStatus(project, itemId, statusName) {
  const option = project.statusField.options.find(
    (candidate) => candidate.name === statusName,
  );
  if (!option) {
    throw new Error(
      `Project status "${statusName}" was not found in field "${config.statusField}"`,
    );
  }

  await graphql(
    env("PROJECT_TOKEN"),
    `
      mutation($project: ID!, $item: ID!, $field: ID!, $option: String!) {
        updateProjectV2ItemFieldValue(
          input: {
            projectId: $project
            itemId: $item
            fieldId: $field
            value: { singleSelectOptionId: $option }
          }
        ) {
          projectV2Item { id }
        }
      }
    `,
    {
      project: project.id,
      item: itemId,
      field: project.statusField.id,
      option: option.id,
    },
  );
}

async function writeIssueContext(issue, runMode) {
  mkdirSync(runtimeDir, { recursive: true });
  const comments = await fetchIssueComments(issue.number);
  const content = [
    `# ${runMode}`,
    "",
    `Repository: ${process.env.GITHUB_REPOSITORY}`,
    `Issue: #${issue.number}`,
    `URL: ${issue.html_url || issue.url}`,
    `Title: ${issue.title}`,
    "",
    "## Issue Body",
    "",
    issue.body || "(empty)",
    "",
    "## Recent Comments",
    "",
    comments.length === 0
      ? "(none)"
      : comments
          .slice(-20)
          .map(
            (comment) =>
              `### ${comment.user.login} at ${comment.created_at}\n\n${comment.body || ""}`,
          )
          .join("\n\n"),
  ].join("\n");
  writeFileSync(`${runtimeDir}/issue-context.md`, content);
}

async function fetchIssueComments(issueNumber) {
  const [owner, repo] = repoParts();
  const response = await rest(
    env("GITHUB_TOKEN"),
    `/repos/${owner}/${repo}/issues/${issueNumber}/comments?per_page=100`,
  );
  return response;
}

async function postIssueComment(issueNumber, body, markerScope) {
  const marker = commentMarker(markerScope, body);
  const comments = await fetchIssueComments(issueNumber);
  if (comments.some((comment) => comment.body?.includes(marker))) {
    return;
  }

  const [owner, repo] = repoParts();
  await rest(env("GITHUB_TOKEN"), `/repos/${owner}/${repo}/issues/${issueNumber}/comments`, {
    method: "POST",
    body: JSON.stringify({ body: `${marker}\n${body}` }),
  });
}

async function graphql(token, query, variables) {
  const response = await fetch(graphqlApi, {
    method: "POST",
    headers: githubHeaders(token),
    body: JSON.stringify({ query, variables }),
  });
  const payload = await response.json();
  if (!response.ok || payload.errors) {
    throw new Error(
      `GitHub GraphQL request failed: ${JSON.stringify(payload.errors || payload)}`,
    );
  }
  return payload.data;
}

async function rest(token, path, options = {}) {
  const response = await fetch(`${githubApi}${path}`, {
    ...options,
    headers: {
      ...githubHeaders(token),
      ...(options.headers || {}),
    },
  });
  if (!response.ok) {
    throw new Error(`GitHub REST ${path} failed: ${response.status} ${await response.text()}`);
  }
  if (response.status === 204) {
    return null;
  }
  return await response.json();
}

function githubHeaders(token) {
  return {
    Authorization: `Bearer ${token}`,
    Accept: "application/vnd.github+json",
    "Content-Type": "application/json",
    "X-GitHub-Api-Version": "2022-11-28",
  };
}

function normalizeContent(content) {
  if (!content) {
    return null;
  }
  return {
    id: content.id,
    type: content.__typename,
    number: content.number,
    title: content.title,
    body: content.body || "",
    state: content.state,
    url: content.url,
    repository: content.repository?.nameWithOwner,
  };
}

function statusValue(item, fieldName) {
  const status = item.fieldValues.nodes.find(
    (value) => value?.field?.name === fieldName,
  );
  return status?.name || "";
}

function renderPullRequestBody(result) {
  const issueNumber = numberEnv("ISSUE_NUMBER");
  return [
    `Refs #${issueNumber}`,
    "",
    "## Summary",
    "",
    result.summary || "Codex completed the requested work.",
    "",
    "## Verification",
    "",
    renderTests(result.tests) || "Not reported.",
    "",
    result.risk ? `## Risk\n\n${result.risk}` : "",
  ]
    .filter(Boolean)
    .join("\n");
}

function renderTests(tests) {
  if (!Array.isArray(tests) || tests.length === 0) {
    return "";
  }
  return tests.map((test) => `- ${test}`).join("\n");
}

function branchName(issueNumber, title) {
  const slug = title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "")
    .slice(0, 48);
  const runId = process.env.GITHUB_RUN_ID || Date.now();
  return `codex/issue-${issueNumber}-${runId}-${slug || "work"}`;
}

function commentMarker(scope, body) {
  const hash = createHash("sha256").update(body).digest("hex").slice(0, 12);
  return `<!-- ash-agent:${scope}:${hash} -->`;
}

function readEvent() {
  return readJson(env("GITHUB_EVENT_PATH"));
}

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function requiredArg(index, name) {
  const value = process.argv[index];
  if (!value) {
    throw new Error(`Missing ${name}`);
  }
  return value;
}

function requiredEnum(value, allowed) {
  if (!allowed.includes(value)) {
    throw new Error(`Expected one of ${allowed.join(", ")}, got ${value}`);
  }
  return value;
}

function isBot(sender) {
  return sender?.type === "Bot" || /\[bot\]$/i.test(sender?.login || "");
}

function repoParts() {
  const repository = env("GITHUB_REPOSITORY");
  const parts = repository.split("/");
  if (parts.length !== 2) {
    throw new Error(`Invalid GITHUB_REPOSITORY: ${repository}`);
  }
  return parts;
}

function env(name) {
  const value = process.env[name];
  if (!value) {
    throw new Error(`Missing required environment variable: ${name}`);
  }
  return value;
}

function numberEnv(name) {
  const value = Number(env(name));
  if (!Number.isInteger(value)) {
    throw new Error(`${name} must be an integer`);
  }
  return value;
}

function setOutput(name, value) {
  const outputPath = process.env.GITHUB_OUTPUT;
  if (!outputPath) {
    console.log(`${name}=${value}`);
    return;
  }
  const delimiter = `EOF_${createHash("sha1").update(`${name}${value}`).digest("hex")}`;
  appendFileSync(outputPath, `${name}<<${delimiter}\n${value}\n${delimiter}\n`);
}
