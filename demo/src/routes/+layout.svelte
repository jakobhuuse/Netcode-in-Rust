<script lang="ts">
	import Header from '$lib/components/layout/Header.svelte';
	import '../app.css';

	let { children } = $props();
</script>

<Header></Header>
{@render children()}
