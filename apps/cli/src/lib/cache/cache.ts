import {
	Clock,
	Duration,
	Effect,
	Cache as EffectCache,
	Exit,
	Option,
} from 'effect';
import type { CacheAdapter, CacheEntry } from './adapter.ts';

// Bundles value with resolved TTL/SWR so the SWR check at read time uses per-entry durations
type CacheValue<Value> = {
	readonly value: Value;
	readonly ttlMs: number;
	readonly swrMs: number;
};

export type CacheInstance<Key, Value, Error> = {
	readonly get: (key: Key) => Effect.Effect<Value, Error>;
	readonly invalidate: (key: Key) => Effect.Effect<void>;
	readonly invalidateAll: Effect.Effect<void>;
};

export namespace Cache {
	export type Entry<Value> = {
		readonly _tag: 'CacheEntry';
		readonly value: Value;
		readonly ttl: Duration.DurationInput;
		readonly swr?: Duration.DurationInput;
	};

	export function entry<Value>(
		value: Value,
		opts: { ttl: Duration.DurationInput; swr?: Duration.DurationInput },
	): Entry<Value> {
		return { _tag: 'CacheEntry', value, ttl: opts.ttl, swr: opts.swr };
	}

	export type LookupResult<Value> = Value | Entry<Value>;

	export function make<Key, Value, Error = never, R = never>(opts: {
		ttl: Duration.DurationInput;
		swr?: Duration.DurationInput;
		lookup: (key: Key) => Effect.Effect<LookupResult<Value>, Error, R>;
		adapter?: CacheAdapter<Key, Value>;
	}): Effect.Effect<CacheInstance<Key, Value, Error>, never, R> {
		return Effect.gen(function* () {
			const adapter = opts.adapter;
			const defaultTtlMs = Duration.toMillis(Duration.decode(opts.ttl));
			const defaultSwrMs = opts.swr
				? Duration.toMillis(Duration.decode(opts.swr))
				: 0;
			const capacity = adapter?.capacity ?? Number.MAX_SAFE_INTEGER;

			// makeWith uses `timeToLive: (exit) => Duration` — the lookup stores CacheValue
			// so timeToLive can extract the total window (ttl + swr) from the exit result
			const inner = yield* EffectCache.makeWith({
				capacity,
				lookup: (key: Key) =>
					Effect.gen(function* () {
						if (adapter) {
							const cached = yield* adapter.get(key);
							if (Option.isSome(cached)) {
								const now = yield* Clock.currentTimeMillis;
								const age = now - cached.value.storedAt;
								const totalWindow = defaultTtlMs + defaultSwrMs;
								if (age < totalWindow) {
									// Adjust remaining TTL/SWR for elapsed age so SWR triggers at correct real-world time
									return {
										value: cached.value.value,
										ttlMs: Math.max(0, defaultTtlMs - age),
										swrMs: Math.max(
											0,
											defaultSwrMs - Math.max(0, age - defaultTtlMs),
										),
									} satisfies CacheValue<Value>;
								}
							}
						}

						const result = yield* opts.lookup(key);
						const cv = resolveLookupResult(result, defaultTtlMs, defaultSwrMs);

						if (adapter) {
							const now = yield* Clock.currentTimeMillis;
							yield* adapter.set(
								key,
								{ value: cv.value, storedAt: now } satisfies CacheEntry<Value>,
								Duration.millis(cv.ttlMs + cv.swrMs),
							);
						}

						return cv;
					}),
				timeToLive: (exit) => {
					if (Exit.isSuccess(exit)) {
						return Duration.millis(exit.value.ttlMs + exit.value.swrMs);
					}
					return Duration.zero;
				},
			});

			// Safe without synchronization — no yield points between has() and add() (cooperative scheduling)
			const refreshingKeys = new Set<string>();

			const get = (key: Key) =>
				Effect.gen(function* () {
					const cv = yield* inner.get(key);

					if (cv.swrMs > 0) {
						const stats = yield* inner.entryStats(key);
						if (Option.isSome(stats)) {
							const now = yield* Clock.currentTimeMillis;
							const age = now - stats.value.loadedMillis;
							if (age > cv.ttlMs) {
								const keyStr = JSON.stringify(key);
								if (!refreshingKeys.has(keyStr)) {
									refreshingKeys.add(keyStr);
									// refresh() recomputes without invalidating, so stale value remains available during recomputation
									yield* Effect.forkDaemon(
										inner.refresh(key).pipe(
											Effect.ensuring(
												Effect.sync(() => {
													refreshingKeys.delete(keyStr);
												}),
											),
											Effect.ignore,
										),
									);
								}
							}
						}
					}

					return cv.value;
				});

			const invalidate = (key: Key) =>
				Effect.gen(function* () {
					yield* inner.invalidate(key);
					if (adapter) yield* adapter.remove(key);
				});

			const invalidateAll = Effect.gen(function* () {
				yield* inner.invalidateAll;
				if (adapter) yield* adapter.removeAll;
			});

			return { get, invalidate, invalidateAll } satisfies CacheInstance<
				Key,
				Value,
				Error
			>;
		});
	}
}

function isCacheEntry<Value>(
	result: Cache.LookupResult<Value>,
): result is Cache.Entry<Value> {
	return (
		typeof result === 'object' &&
		result !== null &&
		'_tag' in result &&
		(result as Cache.Entry<Value>)._tag === 'CacheEntry'
	);
}

function resolveLookupResult<Value>(
	result: Cache.LookupResult<Value>,
	defaultTtlMs: number,
	defaultSwrMs: number,
): CacheValue<Value> {
	if (isCacheEntry(result)) {
		return {
			value: result.value,
			ttlMs: Duration.toMillis(Duration.decode(result.ttl)),
			swrMs: result.swr ? Duration.toMillis(Duration.decode(result.swr)) : 0,
		};
	}
	return { value: result, ttlMs: defaultTtlMs, swrMs: defaultSwrMs };
}
