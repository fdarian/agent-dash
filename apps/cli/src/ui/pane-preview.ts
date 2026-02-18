import {
	type CliRenderer,
	type Selection,
	ScrollBoxRenderable,
	TextRenderable,
} from '@opentui/core';
import { parseAnsiToStyledText } from './ansi-parser.ts';
import { PRIMARY_COLOR, UNFOCUSED_COLOR } from './constants.ts';

export function createPanePreview(renderer: CliRenderer) {
	const scrollBox = new ScrollBoxRenderable(renderer, {
		id: 'pane-preview',
		flexGrow: 1,
		border: true,
		title: '[0] Preview',
		stickyScroll: true,
		stickyStart: 'bottom',
		scrollY: true,
	});
	scrollBox.focusable = false;

	const textContent = new TextRenderable(renderer, {
		id: 'pane-preview-content',
		content: '',
	});
	scrollBox.add(textContent);

	const toast = new TextRenderable(renderer, {
		id: 'preview-copy-toast',
		content: ' Copied! ',
		position: 'absolute',
		top: 1,
		right: 2,
		visible: false,
	});
	renderer.root.add(toast);

	let toastTimeout: ReturnType<typeof setTimeout> | null = null;

	function showCopiedToast() {
		toast.visible = true;
		if (toastTimeout) clearTimeout(toastTimeout);
		toastTimeout = setTimeout(() => {
			toast.visible = false;
			toastTimeout = null;
		}, 1500);
	}

	function copyToClipboard(text: string) {
		if (process.platform === 'darwin') {
			Bun.spawn(['pbcopy'], { stdin: new Blob([text]) });
		} else {
			renderer.copyToClipboardOSC52(text);
		}
	}

	renderer.on('selection', (selection: Selection) => {
		if (!selection.selectedRenderables.includes(textContent)) return;
		const text = selection.getSelectedText();
		if (text.length > 0) {
			copyToClipboard(text);
			showCopiedToast();
			queueMicrotask(() => renderer.clearSelection());
		}
	});

	function setFocused(focused: boolean) {
		scrollBox.borderColor = focused ? PRIMARY_COLOR : UNFOCUSED_COLOR;
	}

	function update(content: string) {
		textContent.content = parseAnsiToStyledText(content);
	}

	function scrollBy(amount: number) {
		scrollBox.scrollBy(amount, 'step');
	}

	setFocused(false);

	return { box: scrollBox, update, setFocused, scrollBy };
}
