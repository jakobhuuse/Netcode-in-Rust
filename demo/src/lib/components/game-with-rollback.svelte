<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import Checkbox from './ui/checkbox/checkbox.svelte';
	import Label from './ui/label/label.svelte';

	interface GameProps {
		title?: string;
		serverUrl?: string;
	}

	// Entity representation similar to the server
	interface Entity {
		id: number;
		entity_type: 'Player';
		position: [number, number];
		velocity: [number, number];
		radius: number;
		color: string;
	}

	// Packet definitions
	interface ConnectionRequest {
		type: 'ConnectionRequest';
		client_version: number;
	}

	interface ConnectionAccepted {
		type: 'ConnectionAccepted';
		client_id: number;
	}

	interface PlayerInput {
		type: 'PlayerInput';
		sequence: number;
		timestamp: number;
		input_vector: [number, number];
	}

	interface GameState {
		type: 'GameState';
		timestamp: number;
		last_processed_input: Record<string, number>; // client_id -> sequence
		entities: Entity[];
	}

	type Packet = ConnectionRequest | ConnectionAccepted | PlayerInput | GameState;

	// Props
	let { title = 'Game', serverUrl = 'ws://localhost:8080' }: GameProps = $props();

	// Canvas element
	let canvas = $state<HTMLCanvasElement>();

	// Game state
	let entities = $state<Entity[]>([]);
	let clientId = $state<number | null>(null);
	let connected = $state(false);
	let lastServerTimestamp = $state(0);
	let pingMs = $state(0);
	let socket = $state<WebSocket | null>(null);
	let nextSequence = $state(1);
	let pendingInputs = $state<PlayerInput[]>([]);

	// Game canvas size
	const canvasWidth = 800;
	const canvasHeight = 600;

	// Settings - can be toggled by UI
	let enablePrediction = $state(true);
	let enableReconciliation = $state(true);
	let enableInterpolation = $state(false); // For visual smoothing

	// Movement input state
	let keysPressed = $state(new Set<string>());

	// Client-side prediction physics constants (should match server)
	const PLAYER_SPEED = 200; // pixels per second

	// Connect to the WebSocket server
	function connectToServer() {
		if (socket !== null) {
			socket.close();
		}

		socket = new WebSocket(serverUrl);

		socket.onopen = () => {
			console.log('Connected to server');
			connected = true;

			// Send connection request
			const packet: ConnectionRequest = {
				type: 'ConnectionRequest',
				client_version: 1
			};

			if (socket) {
				socket.send(JSON.stringify(packet));
			}
		};

		socket.onclose = () => {
			console.log('Disconnected from server');
			connected = false;
			clientId = null;
		};

		socket.onerror = (error) => {
			console.error('WebSocket error:', error);
		};

		socket.onmessage = (event) => {
			try {
				const packet = JSON.parse(event.data);
				handleServerPacket(packet);
			} catch (e) {
				console.error('Error parsing server message:', e);
			}
		};
	}

	// Handle incoming server packets
	function handleServerPacket(packet: any) {
		// Convert server packet format to our client format
		if (packet.ConnectionAccepted) {
			const clientIdReceived = packet.ConnectionAccepted.client_id;
			console.log(`Connection accepted, client ID: ${clientIdReceived}`);
			clientId = clientIdReceived;
		} else if (packet.GameState) {
			const gameState = packet.GameState;

			// Calculate ping
			if (gameState.timestamp) {
				const now = Date.now();
				pingMs = now - gameState.timestamp;
			}

			// Store server timestamp
			lastServerTimestamp = gameState.timestamp;

			// Store the latest server state
			let serverEntities = gameState.entities.map((entity: Entity) => ({
				...entity,
				position: [entity.position[0], entity.position[1]] as [number, number],
				velocity: [entity.velocity[0], entity.velocity[1]] as [number, number]
			}));

			// Handle server reconciliation if enabled
			if (enableReconciliation && clientId !== null) {
				const lastProcessedInput = gameState.last_processed_input[clientId.toString()];

				if (lastProcessedInput !== undefined) {
					// Remove older inputs that have been processed by the server
					pendingInputs = pendingInputs.filter((input) => input.sequence > lastProcessedInput);

					// Find our entity
					const myEntity = serverEntities.find((e: Entity) => {
						// Match entity to client - this depends on your server implementation
						// Usually there's a mapping between client ID and entity ID
						return clientId !== null && e.id % 1000 === clientId % 1000; // Simple example mapping
					});

					if (myEntity && pendingInputs.length > 0) {
						// Re-apply all pending inputs
						const myEntityCopy = { ...myEntity };

						// Apply all pending inputs to get the predicted position
						for (const input of pendingInputs) {
							applyInput(myEntityCopy, input.input_vector, 1 / 60); // Assuming 60 ticks/sec
						}

						// Update the entity in the server state
						const index = serverEntities.findIndex((e: Entity) => e.id === myEntity.id);
						if (index !== -1) {
							serverEntities[index] = myEntityCopy;
						}
					}
				}
			}

			// Update entities
			entities = serverEntities;
		}
	}

	// Apply input to an entity (client-side prediction)
	function applyInput(entity: Entity, inputVector: [number, number], deltaTime: number) {
		// Normalize input vector if needed
		const [inputX, inputY] = inputVector;
		const magnitude = Math.sqrt(inputX * inputX + inputY * inputY);

		let normalizedX = 0;
		let normalizedY = 0;

		if (magnitude > 0) {
			normalizedX = inputX / magnitude;
			normalizedY = inputY / magnitude;
		}

		// Update velocity
		entity.velocity = [normalizedX * PLAYER_SPEED, normalizedY * PLAYER_SPEED];

		// Update position
		entity.position[0] += entity.velocity[0] * deltaTime;
		entity.position[1] += entity.velocity[1] * deltaTime;

		// Apply boundary constraints
		entity.position[0] = Math.max(
			entity.radius,
			Math.min(canvasWidth - entity.radius, entity.position[0])
		);
		entity.position[1] = Math.max(
			entity.radius,
			Math.min(canvasHeight - entity.radius, entity.position[1])
		);

		return entity;
	}

	// Send input to server
	function sendInput(inputVector: [number, number]) {
		if (!connected || clientId === null) return;

		// Create input packet
		const input: PlayerInput = {
			type: 'PlayerInput',
			sequence: nextSequence++,
			timestamp: Date.now(),
			input_vector: inputVector
		};

		// Apply client-side prediction if enabled
		if (enablePrediction) {
			// Find our entity
			const myEntity = entities.find((e) => {
				// Match entity to client - simple example mapping
				return clientId !== null && e.id % 1000 === clientId % 1000;
			});

			// If we have an entity, apply the input immediately
			if (myEntity) {
				// Create a copy of our entity and apply the input
				const mutable = { ...myEntity };
				applyInput(mutable, inputVector, 1 / 60); // Assuming 60 ticks/sec

				// Update entity in our local state
				const index = entities.findIndex((e) => e.id === myEntity.id);
				if (index !== -1) {
					entities[index] = mutable;
				}
			}
		}

		// Store this input for reconciliation
		pendingInputs = [...pendingInputs, input];

		// Convert to server packet format
		const packet = {
			PlayerInput: {
				sequence: input.sequence,
				timestamp: input.timestamp,
				input_vector: input.input_vector
			}
		};

		// Send to server
		if (socket && socket.readyState === WebSocket.OPEN) {
			socket.send(JSON.stringify(packet));
		}
	}

	// Process keyboard input and send to server
	function processInput() {
		if (clientId === null) return;

		// Calculate input vector based on keys pressed
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

		// Only send input if there's actual movement
		if (dx !== 0 || dy !== 0) {
			sendInput([dx, dy]);
		}
	}

	// Keyboard event handlers
	function handleKeyDown(event: KeyboardEvent) {
		keysPressed.add(event.key.toLowerCase());
	}

	function handleKeyUp(event: KeyboardEvent) {
		keysPressed.delete(event.key.toLowerCase());
	}

	// Render the game
	function render() {
		if (!canvas) return;

		const ctx = canvas.getContext('2d');
		if (!ctx) return;

		// Clear the canvas
		ctx.fillStyle = 'white';
		ctx.fillRect(0, 0, canvas.width, canvas.height);

		// Draw entities
		for (const entity of entities) {
			ctx.beginPath();
			ctx.arc(entity.position[0], entity.position[1], entity.radius, 0, Math.PI * 2);
			ctx.fillStyle = entity.color;
			ctx.fill();

			// Highlight player's entity
			if (clientId !== null && entity.id % 1000 === clientId % 1000) {
				ctx.strokeStyle = 'black';
				ctx.lineWidth = 2;
				ctx.stroke();

				// Draw velocity vector
				if (entity.velocity[0] !== 0 || entity.velocity[1] !== 0) {
					const startX = entity.position[0];
					const startY = entity.position[1];
					const endX = startX + entity.velocity[0] * 0.1; // Scale for visibility
					const endY = startY + entity.velocity[1] * 0.1;

					ctx.beginPath();
					ctx.moveTo(startX, startY);
					ctx.lineTo(endX, endY);
					ctx.strokeStyle = 'black';
					ctx.lineWidth = 2;
					ctx.stroke();
				}
			}

			// Draw entity ID
			ctx.fillStyle = 'black';
			ctx.font = '12px Arial';
			ctx.textAlign = 'center';
			ctx.fillText(
				entity.id.toString(),
				entity.position[0],
				entity.position[1] - entity.radius - 5
			);
		}

		// Draw game info
		ctx.fillStyle = 'black';
		ctx.font = '14px Arial';
		ctx.textAlign = 'left';
		ctx.fillText(`Client ID: ${clientId || 'Not connected'}`, 10, 20);
		ctx.fillText(`Ping: ${pingMs}ms`, 10, 40);
		ctx.fillText(`Entities: ${entities.length}`, 10, 60);
		ctx.fillText(`Pending inputs: ${pendingInputs.length}`, 10, 80);
	}

	// Game loop
	let lastUpdateTime = 0;
	let animationFrameId = 0;

	function gameLoop(timestamp: number) {
		// Calculate delta time
		const deltaTime = (timestamp - lastUpdateTime) / 1000; // Convert to seconds
		lastUpdateTime = timestamp;

		// Process input
		processInput();

		// Render
		render();

		// Schedule next frame
		animationFrameId = requestAnimationFrame(gameLoop);
	}

	// Initialize game
	onMount(() => {
		// Connect to server
		connectToServer();

		// Set up event listeners
		window.addEventListener('keydown', handleKeyDown);
		window.addEventListener('keyup', handleKeyUp);

		// Start game loop
		lastUpdateTime = performance.now();
		animationFrameId = requestAnimationFrame(gameLoop);

		return () => {
			// Clean up
			window.removeEventListener('keydown', handleKeyDown);
			window.removeEventListener('keyup', handleKeyUp);
			if (typeof window !== 'undefined') {
				cancelAnimationFrame(animationFrameId);
			}

			// Close socket
			if (socket) {
				socket.close();
			}
		};
	});

	onDestroy(() => {
		// Close socket
		if (socket) {
			socket.close();
		}

		// Cancel animation frame
		if (typeof window !== 'undefined') {
			cancelAnimationFrame(animationFrameId);
		}
	});
</script>

<div class="flex flex-col rounded-lg bg-neutral-100 p-4 pt-0">
	<div class="flex items-center justify-between">
		<h1 class="my-2 text-2xl font-bold">{title}</h1>
		<div class="flex">
			<div class="flex items-center gap-2 p-2">
				<Checkbox
					id="prediction"
					bind:checked={enablePrediction}
					aria-labelledby="prediction-label"
				/>
				<Label
					id="prediction-label"
					for="prediction"
					class="text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70"
				>
					Prediction
				</Label>
			</div>
			<div class="flex items-center gap-2 p-2">
				<Checkbox
					id="reconciliation"
					bind:checked={enableReconciliation}
					aria-labelledby="reconciliation-label"
				/>
				<Label
					id="reconciliation-label"
					for="reconciliation"
					class="text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70"
				>
					Reconciliation
				</Label>
			</div>
			<div class="flex items-center gap-2 p-2">
				<Checkbox
					id="interpolation"
					bind:checked={enableInterpolation}
					aria-labelledby="interpolation-label"
				/>
				<Label
					id="interpolation-label"
					for="interpolation"
					class="text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70"
				>
					Interpolation
				</Label>
			</div>
		</div>
	</div>
	<div class="overflow-hidden rounded-lg">
		<canvas
			bind:this={canvas}
			width={canvasWidth}
			height={canvasHeight}
			class="border border-gray-300"
		></canvas>
	</div>
	<div class="mt-2 text-sm">
		<p>Use WASD or arrow keys to move your circle.</p>
		{#if !connected}
			<p class="text-red-500">Not connected to server. Trying to connect to {serverUrl}...</p>
		{:else}
			<p class="text-green-500">Connected to server! Your client ID: {clientId}</p>
		{/if}
	</div>
</div>
