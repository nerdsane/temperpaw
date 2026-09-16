interface AgentSetupStatus {
  has_anthropic_key: boolean;
  has_agents: boolean;
  has_personalized_soul: boolean;
}

export function requiresAgentSetup(path: string, setup: AgentSetupStatus): boolean {
  return path !== '/welcome' && path !== '/foresight' && path !== '/settings'
    && (!setup.has_anthropic_key || !setup.has_agents || !setup.has_personalized_soul);
}
