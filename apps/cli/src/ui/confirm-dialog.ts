import {
	BoxRenderable,
	type CliRenderer,
	TextRenderable,
} from '@opentui/core';
import { PRIMARY_COLOR } from './constants.ts';

export function createConfirmDialog(renderer: CliRenderer, backgroundColor: string) {
	const modal = new BoxRenderable(renderer, {
		id: 'confirm-modal',
		position: 'absolute',
		zIndex: 110,
		width: '40%',
		height: 5,
		top: '45%',
		left: '30%',
		border: true,
		borderColor: PRIMARY_COLOR,
		title: 'Confirm',
		flexDirection: 'column',
		paddingX: 1,
		backgroundColor,
		visible: false,
		justifyContent: 'center',
		alignItems: 'center',
	});

	const messageText = new TextRenderable(renderer, {
		id: 'confirm-message',
		content: '',
		fg: '#CCCCCC',
	});
	modal.add(messageText);

	const hintText = new TextRenderable(renderer, {
		id: 'confirm-hint',
		content: '[Enter] Confirm  [Esc] Cancel',
		fg: '#666666',
	});
	modal.add(hintText);

	let visible = false;
	let pendingPaneTarget = '';

	function show(paneTarget: string, label: string) {
		pendingPaneTarget = paneTarget;
		messageText.content = `Close session ${label}?`;
		visible = true;
		modal.visible = true;
	}

	function hide() {
		visible = false;
		modal.visible = false;
		pendingPaneTarget = '';
	}

	function getIsVisible() {
		return visible;
	}

	function getPendingPaneTarget() {
		return pendingPaneTarget;
	}

	return { modal, show, hide, getIsVisible, getPendingPaneTarget };
}
