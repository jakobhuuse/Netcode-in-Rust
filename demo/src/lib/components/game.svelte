<script lang="ts">
	import Checkbox from './ui/checkbox/checkbox.svelte';
	import Input from './ui/input/input.svelte';
	import Label from './ui/label/label.svelte';

	interface GameProps {
		title?: string;
	}

	interface Player {
		id: string;
		x: number;
		y: number;
		radius: number;
		color: string;
	}

	let { title = 'Game' }: GameProps = $props();

	let canvas = $state<HTMLCanvasElement>();
	let prediction = $state(false);
	let reconciliation = $state(false);
	let interpolation = $state(false);

	let players = $state<Player[]>([
		{ id: 'player1', x: 100, y: 100, radius: 20, color: 'blue' },
		{ id: 'player2', x: 200, y: 150, radius: 25, color: 'red' }
	]);

	const CONTROLLED_PLAYER_ID = 'player1';
	const PLAYER_SPEED = 3; // Pixels per frame

	let keysPressed = $state(new Set<string>());

	function handleKeyDown(event: KeyboardEvent) {
		keysPressed.add(event.key.toLowerCase());
	}

	function handleKeyUp(event: KeyboardEvent) {
		keysPressed.delete(event.key.toLowerCase());
	}

	$effect(() => {
		window.addEventListener('keydown', handleKeyDown);
		window.addEventListener('keyup', handleKeyUp);

		let animationFrameId: number;

		function gameLoop() {
			const playerToControl = players.find((p) => p.id === CONTROLLED_PLAYER_ID);

			if (playerToControl && canvas) {
				const prevX = playerToControl.x;
				const prevY = playerToControl.y;

				let dx = 0;
				let dy = 0;

				const up = keysPressed.has('w') || keysPressed.has('arrowup');
				const down = keysPressed.has('s') || keysPressed.has('arrowdown');
				const left = keysPressed.has('a') || keysPressed.has('arrowleft');
				const right = keysPressed.has('d') || keysPressed.has('arrowright');

				if (up && !down) dy = -1;
				else if (down && !up) dy = 1;

				if (left && !right) dx = -1;
				else if (right && !left) dx = 1;

				if (dx !== 0 || dy !== 0) {
					let currentSpeed = PLAYER_SPEED;
					// Normalize diagonal movement
					if (dx !== 0 && dy !== 0) {
						currentSpeed = PLAYER_SPEED / Math.sqrt(2);
					}
					playerToControl.x += dx * currentSpeed;
					playerToControl.y += dy * currentSpeed;

					playerToControl.x = Math.max(
						playerToControl.radius,
						Math.min(canvas.width - playerToControl.radius, playerToControl.x)
					);
					playerToControl.y = Math.max(
						playerToControl.radius,
						Math.min(canvas.height - playerToControl.radius, playerToControl.y)
					);

					for (const otherPlayer of players) {
						if (otherPlayer.id === CONTROLLED_PLAYER_ID) continue; // Don't check collision with self

						const distX = playerToControl.x - otherPlayer.x;
						const distY = playerToControl.y - otherPlayer.y;
						const distance = Math.sqrt(distX * distX + distY * distY);
						const sumOfRadii = playerToControl.radius + otherPlayer.radius;

						if (distance < sumOfRadii) {
							// Collision detected, revert to previous position
							playerToControl.x = prevX;
							playerToControl.y = prevY;
							break; // No need to check other players if one collision is found
						}
					}
				}
			}
			animationFrameId = requestAnimationFrame(gameLoop);
		}

		animationFrameId = requestAnimationFrame(gameLoop);

		return () => {
			cancelAnimationFrame(animationFrameId);
			window.removeEventListener('keydown', handleKeyDown);
			window.removeEventListener('keyup', handleKeyUp);
			keysPressed.clear();
		};
	});

	$effect(() => {
		if (canvas) {
			const ctx = canvas.getContext('2d');
			if (ctx) {
				ctx.fillStyle = 'white';
				ctx.fillRect(0, 0, canvas.width, canvas.height);

				for (const player of players) {
					ctx.beginPath();
					ctx.arc(player.x, player.y, player.radius, 0, Math.PI * 2);
					ctx.fillStyle = player.color;
					ctx.fill();
					ctx.closePath();
				}
			}
		}
	});
</script>

<div class="flex flex-col rounded-lg bg-neutral-100 p-4 pt-0">
	<div class="flex items-center justify-between">
		<h1 class="my-2 text-2xl font-bold">{title}</h1>
		<div class="flex">
			<div class="flex items-center gap-2 p-2">
				<Checkbox id="prediction" aria-labelledby="prediction-label" />
				<Label
					id="prediction-label"
					for="prediction"
					class="text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70"
				>
					Prediction
				</Label>
			</div>
			<div class="flex items-center gap-2 p-2">
				<Checkbox id="reconciliation" aria-labelledby="reconciliation-label" />
				<Label
					id="reconciliation-label"
					for="reconciliation"
					class="text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70"
				>
					reconciliation
				</Label>
			</div>
			<div class="flex items-center gap-2 p-2">
				<Checkbox id="interpolation" aria-labelledby="interpolation-label" />
				<Label
					id="interpolation-label"
					for="interpolation"
					class="text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70"
				>
					interpolation
				</Label>
			</div>
		</div>
	</div>
	<div class="overflow-hidden rounded-lg">
		<canvas bind:this={canvas} width="600" height="600"></canvas>
	</div>
</div>
