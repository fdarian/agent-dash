import { Command } from '@effect/cli';
import { BunContext, BunRuntime } from '@effect/platform-bun';
import { Effect } from 'effect';
import { App } from '../src/ui/app.ts';
import { TmuxClient } from '../src/services/tmux-client.ts';
import pkg from '../package.json' with { type: 'json' };

const agentDashCmd = Command.make('agent-dash', {}, () =>
	App.pipe(Effect.provide(TmuxClient.Default)),
);

export const cli = Command.run(agentDashCmd, {
	name: 'agent-dash',
	version: pkg.version,
});

cli(process.argv).pipe(
	Effect.provide(BunContext.layer),
	BunRuntime.runMain,
);
