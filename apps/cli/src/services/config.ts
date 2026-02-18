import { Effect } from 'effect';
import { join } from 'path';
import { homedir } from 'os';

const CONFIG_FILE = join(homedir(), '.config', 'agent-dash', 'config.json');

type ConfigFile = {
	sessionNameFormatter?: string;
	command?: string;
};

const DEFAULT_COMMAND = 'claude';

function expandTilde(filePath: string) {
	if (filePath.startsWith('~/')) {
		return join(homedir(), filePath.slice(2));
	}
	return filePath;
}

function loadConfigFile() {
	return Effect.gen(function* () {
		const raw = yield* Effect.tryPromise(() =>
			Bun.file(CONFIG_FILE).text(),
		).pipe(Effect.catchAll(() => Effect.succeed(null)));

		if (raw === null) return null;

		return yield* Effect.try(() => JSON.parse(raw) as ConfigFile).pipe(
			Effect.catchAll(() => Effect.succeed(null)),
		);
	});
}

function createFormatSessionName(formatterPath: string | undefined) {
	if (formatterPath === undefined) {
		return (name: string) => Effect.succeed(name);
	}

	const resolvedPath = expandTilde(formatterPath);
	const cache = new Map<string, string>();

	return (name: string) =>
		Effect.gen(function* () {
			const cached = cache.get(name);
			if (cached !== undefined) return cached;

			const result = yield* Effect.tryPromise(async () => {
				const proc = Bun.spawn([resolvedPath, name], {
					stdout: 'pipe',
					stderr: 'pipe',
				});
				const exitCode = await proc.exited;
				if (exitCode !== 0) throw new Error('Formatter exited with non-zero');
				return (await new Response(proc.stdout).text()).trim();
			}).pipe(Effect.catchAll(() => Effect.succeed(name)));

			cache.set(name, result);
			return result;
		});
}

export class AppConfig extends Effect.Service<AppConfig>()('AppConfig', {
	effect: Effect.gen(function* () {
		const config = yield* loadConfigFile();

		const command = config?.command ?? DEFAULT_COMMAND;
		const formatSessionName = createFormatSessionName(
			config?.sessionNameFormatter,
		);

		return { command, formatSessionName };
	}),
}) {}
