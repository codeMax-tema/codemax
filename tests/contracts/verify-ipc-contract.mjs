import { readFileSync } from 'node:fs';
import process from 'node:process';

const root = process.cwd();

function read(path) {
  return readFileSync(`${root}/${path}`, 'utf8');
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

function sortedUnique(values) {
  return [...new Set(values)].sort();
}

function difference(left, right) {
  const rightSet = new Set(right);
  return left.filter((value) => !rightSet.has(value));
}

function assertSameSet(label, left, right) {
  const leftOnly = difference(left, right);
  const rightOnly = difference(right, left);
  assert(
    leftOnly.length === 0 && rightOnly.length === 0,
    `${label} mismatch\nleft only: ${leftOnly.join(', ') || '(none)'}\nright only: ${rightOnly.join(', ') || '(none)'}`,
  );
}

const rustSource = read('apps/desktop/src-tauri/src/lib.rs');
const clientSource = read('apps/desktop/src/api/tauriClient.ts');
const schema = JSON.parse(read('contracts/ipc.schema.json'));
const agentApiSchema = JSON.parse(read('contracts/agent-api.schema.json'));

const rustCommands = sortedUnique(
  [...rustSource.matchAll(/commands::[A-Za-z0-9_]+::([A-Za-z0-9_]+)/g)].map(
    (match) => match[1],
  ),
);
const clientCommands = sortedUnique(
  [...clientSource.matchAll(/invokeCommand(?:<[^()]*>)?\(\s*'([a-z0-9_]+)'/g)].map(
    (match) => match[1],
  ),
);
const schemaCommands = sortedUnique(Object.keys(schema.properties.commands.properties));
const requiredSchemaCommands = sortedUnique(schema.properties.commands.required);

assertSameSet('Rust handlers and IPC schema commands', rustCommands, schemaCommands);
assertSameSet('Rust handlers and TypeScript client commands', rustCommands, clientCommands);
assertSameSet('IPC schema properties and required commands', schemaCommands, requiredSchemaCommands);

const strictSharedDefinitions = [
  'AgentStateSnapshot',
  'TaskDetail',
  'ModelConfigView',
  'ActiveProfileView',
  'ProfileCreateRequest',
  'ProfileUpdateRequest',
  'TaskStartPreviewRequest',
  'PrivacyPreview',
  'RunContractPreview',
  'RunContractView',
  'PrivacyLedgerEntry',
  'PrivacyLedgerSummary',
  'TokenBudgetSummary',
  'ContextSource',
  'ContractBreachRecord',
  'RecordContractBreachRequest',
  'TaskMemoryUsage',
  'RecordMemoryUsageRequest',
  'PreferenceCandidatesRequest',
  'PreferenceCandidate',
  'CreatePreferenceCandidateRequest',
  'DecidePreferenceCandidateRequest',
  'GeneratedTaskDelivery',
  'GeneratedTaskProofPack',
  'QualityGateResultState',
  'DeliveryScoreState',
  'DeliveryReviewState',
  'QualityGateRecord',
  'QualityGateOverrideResult',
  'RuleHitRecord',
  'HookRunRecord',
  'HookApprovalRecord',
  'ModelArenaDecisionRecord',
];

for (const definitionName of strictSharedDefinitions) {
  const definition = schema.$defs[definitionName];
  assert(definition, `IPC schema is missing shared definition: ${definitionName}`);
  assert(definition.type === 'object', `${definitionName} must be an object schema`);
  assert(
    definition.additionalProperties === false,
    `${definitionName} must set additionalProperties to false`,
  );
  assert(
    definition.properties && Object.keys(definition.properties).length > 0,
    `${definitionName} must declare concrete properties`,
  );
  assert(
    Array.isArray(definition.required),
    `${definitionName} must declare its required fields`,
  );
  assertNoExplicitOpenObjects(definition, definitionName);
}

function assertNoExplicitOpenObjects(value, path) {
  if (!value || typeof value !== 'object') {
    return;
  }

  assert(
    value.additionalProperties !== true,
    `${path} must not use additionalProperties: true`,
  );

  for (const [key, child] of Object.entries(value)) {
    assertNoExplicitOpenObjects(child, `${path}.${key}`);
  }
}

function assertResolvableRefs(value, path = 'schema') {
  if (!value || typeof value !== 'object') {
    return;
  }

  if (typeof value.$ref === 'string') {
    const prefix = '#/$defs/';
    assert(value.$ref.startsWith(prefix), `${path} has unsupported ref: ${value.$ref}`);
    const definitionName = value.$ref.slice(prefix.length);
    assert(schema.$defs[definitionName], `${path} has unresolved ref: ${value.$ref}`);
  }

  for (const [key, child] of Object.entries(value)) {
    assertResolvableRefs(child, `${path}.${key}`);
  }
}

assertResolvableRefs(schema);

assert(
  schema.properties.commands.properties.estimate_task_workspace,
  'IPC schema must include estimate_task_workspace',
);

const repositorySummary = schema.$defs.RepositorySummary;
assert(repositorySummary.properties.isGitRepository, 'RepositorySummary must include isGitRepository');
assert(
  repositorySummary.properties.branch.anyOf?.some((entry) => entry.type === 'null'),
  'RepositorySummary.branch must accept null for non-Git directories',
);

const createTaskRequest = schema.$defs.CreateTaskRecordRequest;
for (const field of [
  'mode',
  'reasoningEffort',
  'permissionLevel',
  'networkPolicy',
  'workMode',
  'workspaceStrategy',
  'originalWriteAuthorized',
  'workspaceExclusions',
]) {
  assert(createTaskRequest.properties[field], `CreateTaskRecordRequest must include ${field}`);
}

const taskSummary = schema.$defs.TaskSummary;
for (const field of ['workspaceKind', 'sourcePath', 'originalWriteAuthorized', 'workspaceEstimatedBytes']) {
  assert(taskSummary.properties[field], `TaskSummary must include ${field}`);
  assert(taskSummary.required.includes(field), `TaskSummary must require ${field}`);
}

const toolResultEndpoint = agentApiSchema.properties.endpoints.properties.submitAgentToolResult;
assert(toolResultEndpoint, 'Agent API schema must include submitAgentToolResult');
assert(toolResultEndpoint.properties.method.const === 'POST', 'submitAgentToolResult must use POST');
assert(
  toolResultEndpoint.properties.path.const === '/api/v1/tasks/{taskId}/tool-result',
  'submitAgentToolResult must target the tool-result route',
);
assert(
  toolResultEndpoint.properties.request.$ref === '#/$defs/AgentToolResultRequest',
  'submitAgentToolResult must reference AgentToolResultRequest',
);
assert(
  toolResultEndpoint.properties.response.$ref === '#/$defs/AdvanceAgentTaskResponse',
  'submitAgentToolResult must return AdvanceAgentTaskResponse',
);

const agentToolResultRequest = agentApiSchema.$defs.AgentToolResultRequest;
assert(agentToolResultRequest, 'Agent API schema must define AgentToolResultRequest');
assert(
  agentToolResultRequest.type === 'object',
  'AgentToolResultRequest must be an object schema',
);
assert(
  agentToolResultRequest.additionalProperties === false,
  'AgentToolResultRequest must reject unknown properties',
);
for (const field of [
  'callId',
  'toolName',
  'status',
  'output',
  'errorCode',
  'errorMessage',
  'artifactRefs',
  'truncated',
]) {
  assert(
    agentToolResultRequest.properties[field],
    `AgentToolResultRequest must include ${field}`,
  );
}

for (const field of ['callId', 'toolName', 'status']) {
  assert(
    agentToolResultRequest.required.includes(field),
    `AgentToolResultRequest must require ${field}`,
  );
}
for (const field of ['output', 'artifactRefs', 'truncated']) {
  assert(
    !agentToolResultRequest.required.includes(field),
    `AgentToolResultRequest must default ${field} when omitted`,
  );
}
assert(
  JSON.stringify(agentToolResultRequest.properties.output.default) === '{}',
  'AgentToolResultRequest.output must default to an empty object',
);
assert(
  JSON.stringify(agentToolResultRequest.properties.artifactRefs.default) === '[]',
  'AgentToolResultRequest.artifactRefs must default to an empty array',
);
assert(
  agentToolResultRequest.properties.truncated.default === false,
  'AgentToolResultRequest.truncated must default to false',
);

for (const field of ['callId', 'toolName']) {
  const identifier = agentToolResultRequest.properties[field];
  assert(identifier.minLength === 1, `${field} must have a minimum length of 1`);
  assert(identifier.maxLength === 256, `${field} must be limited to 256 characters`);
  assert(identifier.pattern === '\\S', `${field} must reject whitespace-only identifiers`);
  const pattern = new RegExp(identifier.pattern);
  assert(!pattern.test(' \t\n'), `${field} pattern must reject whitespace-only values`);
  assert(pattern.test('tool-call'), `${field} pattern must accept non-blank values`);
}

const errorCode = agentToolResultRequest.properties.errorCode;
assert(errorCode.maxLength === 256, 'errorCode must be limited to 256 characters');
const errorMessage = agentToolResultRequest.properties.errorMessage;
assert(errorMessage.maxLength === 16000, 'errorMessage must be limited to 16000 characters');

const artifactRefs = agentToolResultRequest.properties.artifactRefs;
assert(artifactRefs.maxItems === 16, 'artifactRefs must contain at most 16 values');
assert(artifactRefs.items.minLength === 1, 'artifact references must not be empty');
assert(
  artifactRefs.items.maxLength === 4096,
  'artifact references must be limited to 4096 characters',
);

const output = agentToolResultRequest.properties.output;
assert(output.maxProperties === 256, 'output must limit top-level properties');
assert(
  output.additionalProperties.$ref === '#/$defs/JsonValue',
  'output values must use bounded recursive JSON values',
);
assert(
  agentApiSchema.$defs.JsonValue.$ref === '#/$defs/JsonValueDepth2',
  'JsonValue must begin at the second output nesting level',
);

for (let depth = 2; depth <= 12; depth += 1) {
  const definitionName = `JsonValueDepth${depth}`;
  const jsonValue = agentApiSchema.$defs[definitionName];
  assert(jsonValue, `Agent API schema must define ${definitionName}`);
  assert(Array.isArray(jsonValue.anyOf), `${definitionName} must be a JSON value union`);
  assert(
    jsonValue.anyOf.some((entry) => entry.type === 'array'),
    `${definitionName} must support nested JSON arrays`,
  );
  assert(
    jsonValue.anyOf.some((entry) => entry.type === 'object'),
    `${definitionName} must support nested JSON objects`,
  );

  const array = jsonValue.anyOf.find((entry) => entry.type === 'array');
  const object = jsonValue.anyOf.find((entry) => entry.type === 'object');

  if (depth < 12) {
    assert(array.maxItems === 256, `${definitionName} arrays must limit item counts`);
    assert(object.maxProperties === 256, `${definitionName} objects must limit property counts`);
    const nextDefinition = `#/$defs/JsonValueDepth${depth + 1}`;
    assert(
      array.items.$ref === nextDefinition,
      `${definitionName} arrays must advance the nesting boundary`,
    );
    assert(
      object.additionalProperties.$ref === nextDefinition,
      `${definitionName} objects must advance the nesting boundary`,
    );
  } else {
    assert(array.maxItems === 0, 'JsonValueDepth12 arrays must prevent deeper nesting');
    assert(object.maxProperties === 0, 'JsonValueDepth12 objects must prevent deeper nesting');
  }
}

const toolResultResponse = agentApiSchema.$defs.AdvanceAgentTaskResponse;
assert(
  toolResultResponse.additionalProperties === false,
  'submitAgentToolResult response must reject unknown properties',
);

console.log(
  `IPC contract verified: ${rustCommands.length} commands and ${strictSharedDefinitions.length} strict shared definitions.`,
);
