import { Command, Options } from '@effect/cli';
import * as BunContext from '@effect/platform-bun/BunContext';
import * as BunRuntime from '@effect/platform-bun/BunRuntime';
import * as Effect from 'effect/Effect';
import * as Layer from 'effect/Layer';
import pkg from '../package.json' with { type: 'json' };
import { AppConfig } from '../src/services/config.ts';
import { TmuxClient } from '../src/services/tmux-client.ts';
import { App } from '../src/ui/app.ts';

const AppLayer = TmuxClient.Default.pipe(
	Layer.provideMerge(AppConfig.Default),
);

const exit = Options.boolean('exit').pipe(Options.withDefault(false));
const bench = Options.boolean('bench').pipe(Options.withDefault(false));

const agentDashCmd = Command.make('agent-dash', { exit, bench }, () =>
	App.pipe(Effect.provide(AppLayer)),
);

export const cli = Command.run(agentDashCmd, {
	name: 'agent-dash',
	version: pkg.version,
});

cli(process.argv).pipe(Effect.provide(BunContext.layer), BunRuntime.runMain);
