import { Data, Effect } from 'effect';
import { type ClaudeSession, parseSessionStatus } from '../domain/session.ts';

export class TmuxError extends Data.TaggedError('TmuxError')<{
	message: string;
	command: string;
}> {}

export interface CreatedPaneInfo {
	paneId: string;
	panePid: string;
	paneTitle: string;
	paneTarget: string;
	sessionName: string;
}

export class TmuxClient extends Effect.Service<TmuxClient>()('TmuxClient', {
	succeed: {
		discoverSessions: Effect.gen(function* () {
			const format = [
				'#{pane_id}',
				'#{pane_pid}',
				'#{pane_title}',
				'#{session_name}:#{window_index}.#{pane_index}',
			].join('\t');

			const output = yield* runCommand('tmux', [
				'list-panes',
				'-a',
				'-F',
				format,
			]);

			const lines = output.trim().split('\n').filter(Boolean);
			const sessions: Array<ClaudeSession> = [];

			for (const line of lines) {
				const parts = line.split('\t');
				if (parts.length < 4) continue;

				const paneId = parts[0];
				const panePid = parts[1];
				const paneTitle = parts[2];
				const paneTarget = parts[3];
				if (!paneId || !panePid || !paneTitle || !paneTarget) continue;

				const sessionName = paneTarget.split(':')[0];
				if (!sessionName) continue;

				const isClaude = yield* checkForClaudeProcess(panePid);
				if (!isClaude) continue;

				sessions.push({
					paneId,
					paneTarget,
					title: paneTitle,
					sessionName,
					status: parseSessionStatus(paneTitle),
				});
			}

			return sessions;
		}),

		capturePaneContent: (paneTarget: string) =>
			runCommand('tmux', ['capture-pane', '-e', '-t', paneTarget, '-p', '-S', '-']),

		switchToPane: (paneTarget: string) =>
			runCommand('tmux', ['switch-client', '-t', paneTarget]).pipe(Effect.asVoid),

		openPopup: (paneTarget: string) =>
			runCommand('tmux', [
				'display-popup',
				'-E',
				'-w',
				'80%',
				'-h',
				'80%',
				`tmux capture-pane -S - -e -p -t ${paneTarget} | less -R`,
			]).pipe(Effect.asVoid),

		startPipePane: (paneTarget: string) => {
			const tmpFile = `/tmp/agent-dash-pipe-${paneTarget.replace(/[^a-zA-Z0-9]/g, '-')}`;
			return runCommand('tmux', [
				'pipe-pane',
				'-O',
				'-t',
				paneTarget,
				`cat >> ${tmpFile}`,
			]).pipe(Effect.as(tmpFile));
		},

		stopPipePane: (paneTarget: string) =>
			runCommand('tmux', ['pipe-pane', '-t', paneTarget]).pipe(Effect.asVoid),

		createWindow: (sessionName: string) =>
			Effect.gen(function* () {
				const format = [
					'#{pane_id}',
					'#{pane_pid}',
					'#{pane_title}',
					'#{session_name}:#{window_index}.#{pane_index}',
				].join('\t');

				const output = yield* runCommand('tmux', [
					'new-window',
					'-d',
					'-P',
					'-F',
					format,
					'-t',
					sessionName,
					'claude',
				]);

				const parts = output.trim().split('\t');
				if (parts.length < 4) return undefined;

				const paneId = parts[0];
				const panePid = parts[1];
				const paneTitle = parts[2];
				const paneTarget = parts[3];
				if (!paneId || !panePid || !paneTarget) return undefined;

				const parsedSessionName = paneTarget.split(':')[0];
				if (!parsedSessionName) return undefined;

				return {
					paneId,
					panePid,
					paneTitle: paneTitle ?? '',
					paneTarget,
					sessionName: parsedSessionName,
				} satisfies CreatedPaneInfo;
			}),

		killPane: (paneTarget: string) =>
			runCommand('tmux', ['kill-pane', '-t', paneTarget]).pipe(Effect.asVoid),
	},
}) {}

function runCommand(
	cmd: string,
	args: Array<string>,
): Effect.Effect<string, TmuxError> {
	return Effect.tryPromise({
		try: async () => {
			const proc = Bun.spawn([cmd, ...args], {
				stdout: 'pipe',
				stderr: 'pipe',
			});
			const exitCode = await proc.exited;
			const stdout = await new Response(proc.stdout).text();
			const stderr = await new Response(proc.stderr).text();
			if (exitCode !== 0) {
				throw new Error(stderr || `Process exited with code ${exitCode}`);
			}
			return stdout;
		},
		catch: (error) =>
			new TmuxError({
				message: error instanceof Error ? error.message : String(error),
				command: `${cmd} ${args.join(' ')}`,
			}),
	});
}

function checkForClaudeProcess(
	parentPid: string,
): Effect.Effect<boolean, TmuxError> {
	return Effect.gen(function* () {
		const selfComm = yield* runCommand('ps', [
			'-o',
			'comm=',
			'-p',
			parentPid,
		]).pipe(Effect.catchAll(() => Effect.succeed('')));
		if (selfComm.trim().endsWith('claude')) return true;

		const pgrepOutput = yield* runCommand('pgrep', ['-P', parentPid]).pipe(
			Effect.catchAll(() => Effect.succeed('')),
		);

		const childPids = pgrepOutput.trim().split('\n').filter(Boolean);

		for (const childPid of childPids) {
			const comm = yield* runCommand('ps', [
				'-o',
				'comm=',
				'-p',
				childPid,
			]).pipe(Effect.catchAll(() => Effect.succeed('')));
			if (comm.trim().endsWith('claude')) return true;

			const nested = yield* checkForClaudeProcess(childPid);
			if (nested) return true;
		}

		return false;
	});
}
