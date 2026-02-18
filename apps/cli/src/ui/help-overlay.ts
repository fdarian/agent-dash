import {
	BoxRenderable,
	type CliRenderer,
	InputRenderable,
	ScrollBoxRenderable,
	TextRenderable,
} from '@opentui/core';
import { filterKeybinds } from './keybinds.ts';
import { PRIMARY_COLOR } from './constants.ts';

export function createHelpOverlay(renderer: CliRenderer) {
	const backdrop = new BoxRenderable(renderer, {
		id: 'help-backdrop',
		position: 'absolute',
		width: '100%',
		height: '100%',
		zIndex: 90,
		backgroundColor: '#000000',
		opacity: 0.5,
		visible: false,
	});

	const modal = new BoxRenderable(renderer, {
		id: 'help-modal',
		position: 'absolute',
		zIndex: 100,
		width: 50,
		height: 20,
		top: '20%',
		left: '25%',
		border: true,
		borderColor: PRIMARY_COLOR,
		title: 'Help - Keybinds',
		flexDirection: 'column',
		paddingX: 1,
		paddingY: 1,
		backgroundColor: '#1E1E1E',
		visible: false,
	});

	const filterInput = new InputRenderable(renderer, {
		id: 'help-filter-input',
		placeholder: 'Type to filter...',
		placeholderColor: '#666666',
		width: '100%',
	});
	modal.add(filterInput);

	const keybindList = new ScrollBoxRenderable(renderer, {
		id: 'help-keybind-list',
		flexGrow: 1,
		scrollY: true,
		marginTop: 1,
	});
	modal.add(keybindList);

	let listChildIds: Array<string> = [];

	function renderKeybinds(query: string) {
		for (const id of listChildIds) {
			keybindList.remove(id);
		}
		listChildIds = [];

		const entries = filterKeybinds(query);
		for (let i = 0; i < entries.length; i++) {
			const entry = entries[i];
			if (entry === undefined) continue;

			const id = `help-keybind-${i}`;
			const keyPadded = entry.key.padEnd(8);
			const text = new TextRenderable(renderer, {
				id,
				content: `${keyPadded} ${entry.description}`,
				fg: '#CCCCCC',
			});
			keybindList.add(text);
			listChildIds.push(id);
		}

		if (entries.length === 0) {
			const id = 'help-keybind-empty';
			const text = new TextRenderable(renderer, {
				id,
				content: 'No matching keybinds',
				fg: '#666666',
			});
			keybindList.add(text);
			listChildIds.push(id);
		}
	}

	(filterInput as unknown as NodeJS.EventEmitter).on('input', () => {
		renderKeybinds(filterInput.value);
	});

	renderKeybinds('');

	let visible = false;

	function show() {
		visible = true;
		backdrop.visible = true;
		modal.visible = true;
		filterInput.value = '';
		renderKeybinds('');
		filterInput.focus();
	}

	function hide() {
		visible = false;
		backdrop.visible = false;
		modal.visible = false;
		filterInput.blur();
	}

	function toggle() {
		if (visible) {
			hide();
		} else {
			show();
		}
	}

	function getIsVisible() {
		return visible;
	}

	return { backdrop, modal, show, hide, toggle, getIsVisible };
}
