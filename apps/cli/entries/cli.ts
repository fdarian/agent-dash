import { Command } from '@effect/cli';
import { BunContext, BunRuntime } from '@effect/platform-bun';
import { Effect, Layer } from 'effect';
import pkg from '../package.json' with { type: 'json' };
import { AppConfig } from '../src/services/config.ts';
import { TmuxClient } from '../src/services/tmux-client.ts';
import { App } from '../src/ui/app.ts';

const AppLayer = TmuxClient.Default.pipe(
	Layer.provideMerge(AppConfig.Default),
);

const agentDashCmd = Command.make('agent-dash', {}, () =>
	App.pipe(Effect.provide(AppLayer)),
);

export const cli = Command.run(agentDashCmd, {
	name: 'agent-dash',
	version: pkg.version,
});

cli(process.argv).pipe(Effect.provide(BunContext.layer), BunRuntime.runMain);
