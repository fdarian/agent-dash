import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared';

export function baseOptions(): BaseLayoutProps {
  return {
    nav: {
      title: 'Agent Dash',
      url: '/',
    },
    githubUrl: 'https://github.com/fdarian/agent-dash',
  };
}
