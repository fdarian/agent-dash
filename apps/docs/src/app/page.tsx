import type { Metadata } from 'next';
import { LandingPage } from '@/components/landing';

export const metadata: Metadata = {
  title: 'Agent Dash — a terminal instrument for Claude Code sessions',
  description:
    'A keyboard-first terminal dashboard for the Claude Code sessions already running in your tmux server.',
};

export default function HomePage() {
  return <LandingPage />;
}
