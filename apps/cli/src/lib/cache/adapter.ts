import { Duration } from 'effect/Duration';
import { Effect } from 'effect/Effect';
import { Option } from 'effect/Option';
import { mkdir, unlink, rm } from 'fs/promises';

export type CacheEntry<Value> = {
	readonly value: Value;
	readonly storedAt: number;
};

export type CacheAdapter<Key, Value> = {
	readonly get: (key: Key) => Effect.Effect<Option.Option<CacheEntry<Value>>>;
	readonly set: (
		key: Key,
		entry: CacheEntry<Value>,
		ttl: Duration.Duration,
	) => Effect.Effect<void>;
	readonly remove: (key: Key) => Effect.Effect<void>;
	readonly removeAll: Effect.Effect<void>;
	readonly capacity?: number;
};

export namespace CacheAdapter {
	export function memory<Key, Value>(opts?: {
		capacity?: number;
	}): CacheAdapter<Key, Value> {
		return {
			get: () => Effect.succeed(Option.none()),
			set: () => Effect.void,
			remove: () => Effect.void,
			removeAll: Effect.void,
			capacity: opts?.capacity,
		};
	}

	export function fs<Key, Value>(opts: {
		dir: string;
	}): CacheAdapter<Key, Value> {
		const encodeKey = (key: Key) =>
			encodeURIComponent(JSON.stringify(key));

		return {
			get: (key) =>
				Effect.tryPromise({
					try: async () => {
						const file = Bun.file(`${opts.dir}/${encodeKey(key)}.json`);
						const exists = await file.exists();
						if (!exists) return Option.none<CacheEntry<Value>>();
						const entry = (await file.json()) as CacheEntry<Value>;
						return Option.some(entry);
					},
					catch: () => Option.none<CacheEntry<Value>>(),
				}).pipe(Effect.catchAll((e) => Effect.succeed(e))),

			set: (key, entry, _ttl) =>
				Effect.tryPromise({
					try: async () => {
						await mkdir(opts.dir, { recursive: true });
						await Bun.write(
							`${opts.dir}/${encodeKey(key)}.json`,
							JSON.stringify(entry),
						);
					},
					catch: () => undefined,
				}).pipe(Effect.asVoid),

			remove: (key) =>
				Effect.tryPromise({
					try: () => unlink(`${opts.dir}/${encodeKey(key)}.json`),
					catch: () => undefined,
				}).pipe(Effect.asVoid),

			removeAll: Effect.tryPromise({
				try: () => rm(opts.dir, { recursive: true, force: true }),
				catch: () => undefined,
			}).pipe(Effect.asVoid),
		};
	}
}
