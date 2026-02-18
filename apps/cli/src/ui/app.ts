import { BoxRenderable, createCliRenderer, type KeyEvent } from '@opentui/core';
import { Effect, Fiber, Ref, Schedule } from 'effect';
import { parseSessionStatus, type ClaudeSession, type SessionStatus } from '../domain/session.ts';
import { TmuxClient, type CreatedPaneInfo } from '../services/tmux-client.ts';
import { createPanePreview } from './pane-preview.ts';
import { createSessionList } from './session-list.ts';
import { createHelpOverlay } from './help-overlay.ts';
import { createConfirmDialog } from './confirm-dialog.ts';
import { loadState, saveState } from '../services/state.ts';
import { detectTerminalBackground } from '../utils/terminal.ts';
import { groupSessionsByName, buildVisibleItems, resolveSelectedIndex, type VisibleItem } from './session-groups.ts';

export const App = Effect.gen(function* () {
	const terminalBg = yield* detectTerminalBackground;

	const tmux = yield* TmuxClient;

	const renderer = yield* Effect.promise(() =>
		createCliRenderer({
			exitOnCtrlC: true,
			targetFps: 60,
		}),
	);

	const root = new BoxRenderable(renderer, {
		id: 'root',
		flexDirection: 'row',
		width: '100%',
		height: '100%',
	});
	renderer.root.add(root);

	const helpOverlay = createHelpOverlay(renderer, terminalBg);
	renderer.root.add(helpOverlay.modal);

	const confirmDialog = createConfirmDialog(renderer, terminalBg);
	renderer.root.add(confirmDialog.modal);

	const sessionList = createSessionList(renderer);
	const panePreview = createPanePreview(renderer);
	root.add(sessionList.box);
	root.add(panePreview.box);

	const sessionsRef = yield* Ref.make<Array<ClaudeSession>>([]);
	const selectedIndexRef = yield* Ref.make(0);
	const focusRef = yield* Ref.make<'sessions' | 'preview'>('sessions');
	const prevStatusMapRef = yield* Ref.make<Map<string, SessionStatus>>(new Map());
	const unreadPaneIdsRef = yield* Ref.make<Set<string>>(new Set());
	const collapsedGroupsRef = yield* Ref.make<Set<string>>(new Set());
	const visibleItemsRef = yield* Ref.make<Array<VisibleItem>>([]);

	const persistedState = yield* loadState;
	yield* Ref.set(prevStatusMapRef, persistedState.prevStatusMap);
	yield* Ref.set(unreadPaneIdsRef, persistedState.unreadPaneIds);

	const getVisibleItems = Effect.gen(function* () {
		const sessions = yield* Ref.get(sessionsRef);
		const collapsedGroups = yield* Ref.get(collapsedGroupsRef);
		const unreadPaneIds = yield* Ref.get(unreadPaneIdsRef);
		const groups = groupSessionsByName(sessions);
		return buildVisibleItems(groups, collapsedGroups, unreadPaneIds);
	});

	const getSelectedSession = Effect.gen(function* () {
		const visibleItems = yield* Ref.get(visibleItemsRef);
		const selectedIndex = yield* Ref.get(selectedIndexRef);
		const item = visibleItems[selectedIndex];
		if (item !== undefined && item.kind === 'session') {
			return item.session;
		}
		return undefined;
	});

	const refreshSessionListUI = Effect.gen(function* () {
		const visibleItems = yield* getVisibleItems;
		yield* Ref.set(visibleItemsRef, visibleItems);
		const selectedIndex = yield* Ref.get(selectedIndexRef);
		const focus = yield* Ref.get(focusRef);

		const sessionsFocused = focus === 'sessions';
		const previewFocused = focus === 'preview';

		if (sessionsFocused) {
			root.remove(panePreview.box.id);
			root.add(sessionList.box);
			root.add(panePreview.box);
		} else {
			root.remove(sessionList.box.id);
		}

		sessionList.setFocused(sessionsFocused);
		panePreview.setFocused(previewFocused);

		sessionList.update(visibleItems, selectedIndex);
	});

	const refreshPreviewUI = Effect.gen(function* () {
		const selected = yield* getSelectedSession;
		if (selected !== undefined) {
			const content = yield* tmux
				.capturePaneContent(selected.paneTarget)
				.pipe(
					Effect.catchAll(() => Effect.succeed('(unable to capture pane)')),
				);
			panePreview.update(content);
		} else {
			panePreview.update('');
		}
	});

	const pollSessions = Effect.gen(function* () {
		const sessions = yield* tmux.discoverSessions.pipe(
			Effect.catchAll(() => Effect.succeed([] as Array<ClaudeSession>)),
		);
		yield* Ref.set(sessionsRef, sessions);

		const prevStatusMap = yield* Ref.get(prevStatusMapRef);
		const unreadPaneIds = yield* Ref.get(unreadPaneIdsRef);
		const nextStatusMap = new Map<string, SessionStatus>();
		const nextUnreadPaneIds = new Set(unreadPaneIds);
		const currentPaneIds = new Set<string>();

		for (const session of sessions) {
			currentPaneIds.add(session.paneId);
			nextStatusMap.set(session.paneId, session.status);
			const prevStatus = prevStatusMap.get(session.paneId);
			if (prevStatus === 'active' && session.status === 'idle') {
				nextUnreadPaneIds.add(session.paneId);
			}
		}

		for (const paneId of nextUnreadPaneIds) {
			if (!currentPaneIds.has(paneId)) {
				nextUnreadPaneIds.delete(paneId);
			}
		}

		yield* Ref.set(prevStatusMapRef, nextStatusMap);
		yield* Ref.set(unreadPaneIdsRef, nextUnreadPaneIds);
		yield* saveState(nextUnreadPaneIds, nextStatusMap);

		const oldVisibleItems = yield* Ref.get(visibleItemsRef);
		const oldSelectedIndex = yield* Ref.get(selectedIndexRef);
		const newVisibleItems = yield* getVisibleItems;
		const newSelectedIndex = resolveSelectedIndex(newVisibleItems, oldVisibleItems, oldSelectedIndex);
		yield* Ref.set(selectedIndexRef, newSelectedIndex);

		yield* refreshSessionListUI;
	});

	const removeSession = (paneTarget: string) =>
		Effect.gen(function* () {
			const sessions = yield* Ref.get(sessionsRef);
			const removed = sessions.find((s) => s.paneTarget === paneTarget);
			const filtered = sessions.filter((s) => s.paneTarget !== paneTarget);
			yield* Ref.set(sessionsRef, filtered);

			if (removed !== undefined) {
				const prevStatusMap = yield* Ref.get(prevStatusMapRef);
				prevStatusMap.delete(removed.paneId);
				yield* Ref.set(prevStatusMapRef, prevStatusMap);

				const unreadPaneIds = yield* Ref.get(unreadPaneIdsRef);
				unreadPaneIds.delete(removed.paneId);
				yield* Ref.set(unreadPaneIdsRef, unreadPaneIds);
			}

			const unreadPaneIds = yield* Ref.get(unreadPaneIdsRef);
			const prevStatusMap = yield* Ref.get(prevStatusMapRef);
			yield* saveState(unreadPaneIds, prevStatusMap);

			const oldVisibleItems = yield* Ref.get(visibleItemsRef);
			const oldSelectedIndex = yield* Ref.get(selectedIndexRef);
			const newVisibleItems = yield* getVisibleItems;
			const newSelectedIndex = resolveSelectedIndex(newVisibleItems, oldVisibleItems, oldSelectedIndex);
			yield* Ref.set(selectedIndexRef, newSelectedIndex);

			yield* refreshSessionListUI;
		});

	const addSession = (paneInfo: CreatedPaneInfo) =>
		Effect.gen(function* () {
			const session: ClaudeSession = {
				paneId: paneInfo.paneId,
				paneTarget: paneInfo.paneTarget,
				title: paneInfo.paneTitle,
				sessionName: paneInfo.sessionName,
				status: parseSessionStatus(paneInfo.paneTitle),
			};

			const sessions = yield* Ref.get(sessionsRef);
			yield* Ref.set(sessionsRef, [...sessions, session]);

			const prevStatusMap = yield* Ref.get(prevStatusMapRef);
			prevStatusMap.set(session.paneId, session.status);
			yield* Ref.set(prevStatusMapRef, prevStatusMap);

			const unreadPaneIds = yield* Ref.get(unreadPaneIdsRef);
			yield* saveState(unreadPaneIds, prevStatusMap);

			const oldVisibleItems = yield* Ref.get(visibleItemsRef);
			const oldSelectedIndex = yield* Ref.get(selectedIndexRef);
			const newVisibleItems = yield* getVisibleItems;
			const newSelectedIndex = resolveSelectedIndex(newVisibleItems, oldVisibleItems, oldSelectedIndex);
			yield* Ref.set(selectedIndexRef, newSelectedIndex);

			yield* refreshSessionListUI;
		});

	const pollPreview = Effect.gen(function* () {
		yield* refreshPreviewUI;
	});

	const sessionsFiber = yield* pollSessions.pipe(
		Effect.repeat(Schedule.fixed('2 seconds')),
		Effect.fork,
	);
	const previewFiber = yield* pollPreview.pipe(
		Effect.repeat(Schedule.fixed('200 millis')),
		Effect.fork,
	);

	yield* Effect.sync(() => {
		(renderer.keyInput as unknown as NodeJS.EventEmitter).on(
			'keypress',
			(key: KeyEvent) => {
				const handler = Effect.gen(function* () {
					if (confirmDialog.getIsVisible()) {
						if (key.name === 'return') {
							key.preventDefault();
							const paneTarget = confirmDialog.getPendingPaneTarget();
							yield* tmux.killPane(paneTarget).pipe(
								Effect.catchAll(() => Effect.void),
							);
							confirmDialog.hide();
							yield* removeSession(paneTarget);
						} else if (key.name === 'escape') {
							key.preventDefault();
							confirmDialog.hide();
						}
						return;
					}

					if (helpOverlay.getIsVisible()) {
						if (helpOverlay.getIsFilterActive()) {
							if (key.name === 'escape') {
								key.preventDefault();
								helpOverlay.hideFilter();
							}
						} else {
							if (key.name === '?' || key.name === 'escape') {
								key.preventDefault();
								helpOverlay.hide();
							} else if (key.name === '/') {
								key.preventDefault();
								helpOverlay.showFilter();
							}
						}
						return;
					}

					const visibleItems = yield* Ref.get(visibleItemsRef);
					const selectedIndex = yield* Ref.get(selectedIndexRef);
					const focus = yield* Ref.get(focusRef);

					if (key.name === '1') {
						yield* Ref.set(focusRef, 'sessions');
						yield* refreshSessionListUI;
					} else if (key.name === '0') {
						const selected = yield* getSelectedSession;
						if (selected !== undefined) {
							yield* Ref.update(unreadPaneIdsRef, (set) => {
								const next = new Set(set);
								next.delete(selected.paneId);
								return next;
							});
							const updatedUnread = yield* Ref.get(unreadPaneIdsRef);
							const currentStatusMap = yield* Ref.get(prevStatusMapRef);
							yield* saveState(updatedUnread, currentStatusMap);
						}
						yield* Ref.set(focusRef, 'preview');
						yield* refreshSessionListUI;
					} else if (key.name === 'j' || key.name === 'down') {
						if (focus === 'sessions') {
							if (selectedIndex < visibleItems.length - 1) {
								yield* Ref.set(selectedIndexRef, selectedIndex + 1);
								yield* refreshSessionListUI;
								yield* refreshPreviewUI;
							}
						} else if (focus === 'preview') {
							panePreview.scrollBy(1);
						}
					} else if (key.name === 'k' || key.name === 'up') {
						if (focus === 'sessions') {
							if (selectedIndex > 0) {
								yield* Ref.set(selectedIndexRef, selectedIndex - 1);
								yield* refreshSessionListUI;
								yield* refreshPreviewUI;
							}
						} else if (focus === 'preview') {
							panePreview.scrollBy(-1);
						}
					} else if (key.name === 'h') {
						if (focus === 'sessions') {
							const currentItem = visibleItems[selectedIndex];
							if (currentItem !== undefined) {
								if (currentItem.kind === 'group-header') {
									yield* Ref.update(collapsedGroupsRef, (set) => {
										const next = new Set(set);
										next.add(currentItem.sessionName);
										return next;
									});
									yield* refreshSessionListUI;
									yield* refreshPreviewUI;
								} else if (currentItem.kind === 'session') {
									yield* Ref.update(collapsedGroupsRef, (set) => {
										const next = new Set(set);
										next.add(currentItem.groupSessionName);
										return next;
									});
									const updatedVisibleItems = yield* getVisibleItems;
									const headerIndex = updatedVisibleItems.findIndex(
										(item) => item.kind === 'group-header' && item.sessionName === currentItem.groupSessionName,
									);
									if (headerIndex !== -1) {
										yield* Ref.set(selectedIndexRef, headerIndex);
									}
									yield* refreshSessionListUI;
									yield* refreshPreviewUI;
								}
							}
						}
					} else if (key.name === 'l') {
						if (focus === 'sessions') {
							const currentItem = visibleItems[selectedIndex];
							if (currentItem !== undefined && currentItem.kind === 'group-header') {
								yield* Ref.update(collapsedGroupsRef, (set) => {
									const next = new Set(set);
									next.delete(currentItem.sessionName);
									return next;
								});
								yield* refreshSessionListUI;
								yield* refreshPreviewUI;
							}
						}
					} else if (key.name === 'r') {
						const selected = yield* getSelectedSession;
						if (selected !== undefined) {
							yield* Ref.update(unreadPaneIdsRef, (set) => {
								const next = new Set(set);
								next.delete(selected.paneId);
								return next;
							});
							const updatedUnread = yield* Ref.get(unreadPaneIdsRef);
							const currentStatusMap = yield* Ref.get(prevStatusMapRef);
							yield* saveState(updatedUnread, currentStatusMap);
							yield* refreshSessionListUI;
						}
					} else if (key.name === 'o') {
						const selected = yield* getSelectedSession;
						if (selected !== undefined) {
							yield* tmux.switchToPane(selected.paneTarget).pipe(
								Effect.catchAll(() => Effect.void),
							);
						}
					} else if (key.name === 'c') {
						if (focus === 'sessions') {
							const currentItem = visibleItems[selectedIndex];
							if (currentItem !== undefined) {
								const sessionName = currentItem.kind === 'session'
									? currentItem.session.sessionName
									: currentItem.kind === 'group-header'
										? currentItem.sessionName
										: undefined;
								if (sessionName !== undefined) {
									const paneInfo = yield* tmux.createWindow(sessionName).pipe(
										Effect.catchAll(() => Effect.succeed(undefined)),
									);
									yield* tmux.switchToPane(sessionName).pipe(
										Effect.catchAll(() => Effect.void),
									);
									if (paneInfo !== undefined) {
										yield* addSession(paneInfo);
									}
								}
							}
						}
					} else if (key.name === 'x') {
						if (focus === 'sessions') {
							const selected = yield* getSelectedSession;
							if (selected !== undefined) {
								confirmDialog.show(selected.paneTarget, selected.paneTarget);
							}
						}
					} else if (key.name === '?') {
						helpOverlay.toggle();
					} else if (key.name === 'q') {
						renderer.destroy();
					}
				});

				Effect.runPromise(handler).catch(() => {});
			},
		);
	});

	renderer.start();

	yield* Effect.async<void>((resume) => {
		(renderer as unknown as NodeJS.EventEmitter).on('destroy', () => {
			resume(Effect.void);
		});
	});

	yield* Fiber.interrupt(sessionsFiber);
	yield* Fiber.interrupt(previewFiber);
});
