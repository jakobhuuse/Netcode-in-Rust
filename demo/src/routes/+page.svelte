<!-- <script lang="ts">
	import Game from '$lib/components/game.svelte';
</script>

<main class="flex min-h-screen flex-col">
	<div class="flex flex-1 items-center justify-center gap-4">
		<Game title="Client game" />
		<Game title="Server game" />
	</div>
</main> -->

<script lang="ts">
	import GameWithRollback from '$lib/components/game-with-rollback.svelte';
</script>

<main class="flex min-h-screen flex-col">
	<div class="flex flex-1 items-center justify-center">
		<GameWithRollback
			title="Multiplayer Game with Rollback Netcode"
			serverUrl="ws://localhost:8080"
		/>
	</div>
</main>
