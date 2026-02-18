import {
	BoxRenderable,
	type CliRenderer,
	InputRenderable,
	ScrollBoxRenderable,
	TextRenderable,
} from '@opentui/core';
import { filterKeybinds } from './keybinds.ts';
import { PRIMARY_COLOR } from './constants.ts';

export function createHelpOverlay(renderer: CliRenderer, backgroundColor: string) {
	const modal = new BoxRenderable(renderer, {
		id: 'help-modal',
		position: 'absolute',
		zIndex: 100,
		width: '50%',
		height: '60%',
		top: '20%',
		left: '25%',
		border: true,
		borderColor: PRIMARY_COLOR,
		title: 'Help - Keybinds',
		flexDirection: 'column',
		paddingX: 1,
		paddingY: 1,
		backgroundColor,
		visible: false,
	});

	const filterInput = new InputRenderable(renderer, {
		id: 'help-filter-input',
		placeholder: 'Type to filter...',
		placeholderColor: '#666666',
		width: '100%',
	});

	const keybindList = new ScrollBoxRenderable(renderer, {
		id: 'help-keybind-list',
		flexGrow: 1,
		scrollY: true,
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
	let filterActive = false;

	function show() {
		visible = true;
		modal.visible = true;
		filterActive = false;
		filterInput.value = '';
		renderKeybinds('');
	}

	function hide() {
		visible = false;
		filterActive = false;
		modal.visible = false;
		modal.remove(filterInput.id);
		filterInput.blur();
	}

	function toggle() {
		if (visible) {
			hide();
		} else {
			show();
		}
	}

	function showFilter() {
		filterActive = true;
		modal.add(filterInput);
		filterInput.focus();
	}

	function hideFilter() {
		filterActive = false;
		modal.remove(filterInput.id);
		filterInput.blur();
		filterInput.value = '';
		renderKeybinds('');
	}

	function getIsVisible() {
		return visible;
	}

	function getIsFilterActive() {
		return filterActive;
	}

	return { modal, show, hide, toggle, getIsVisible, showFilter, hideFilter, getIsFilterActive };
}
