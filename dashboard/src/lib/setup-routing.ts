interface AgentSetupStatus {
  has_anthropic_key: boolean;
  has_agents: boolean;
  has_personalized_soul: boolean;
}

export function requiresAgentSetup(path: string, setup: AgentSetupStatus): boolean {
  // Auth is checked before this setup guard. Existing session details remain
  // inspectable even when research failed before first-run setup completed.
  const isSessionDetail = /^\/sessions\/[A-Za-z0-9_-]+\/?$/.test(path);
  return !isSessionDetail && path !== '/welcome' && path !== '/foresight' && path !== '/settings'
    && (!setup.has_anthropic_key || !setup.has_agents || !setup.has_personalized_soul);
}
