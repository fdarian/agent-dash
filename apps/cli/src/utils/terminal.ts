import { Effect } from 'effect';

const FALLBACK_COLOR = '#000000';

export const detectTerminalBackground = Effect.promise<string>(() => {
	return new Promise((resolve) => {
		if (!process.stdin.isTTY) {
			resolve(FALLBACK_COLOR);
			return;
		}

		const timeout = setTimeout(() => {
			cleanup();
			resolve(FALLBACK_COLOR);
		}, 300);

		process.stdin.setRawMode(true);
		process.stdin.resume();

		const onData = (data: Buffer) => {
			const response = data.toString();
			const match = response.match(
				/\]11;rgb:([0-9a-fA-F]+)\/([0-9a-fA-F]+)\/([0-9a-fA-F]+)/,
			);
			if (match && match[1] && match[2] && match[3]) {
				clearTimeout(timeout);
				cleanup();
				const r = match[1].substring(0, 2);
				const g = match[2].substring(0, 2);
				const b = match[3].substring(0, 2);
				resolve(`#${r}${g}${b}`);
			}
		};

		const cleanup = () => {
			process.stdin.removeListener('data', onData);
		};

		process.stdin.on('data', onData);
		process.stdout.write('\x1b]11;?\x1b\\');
	});
});
