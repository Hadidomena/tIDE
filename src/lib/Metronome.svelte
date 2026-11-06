<script lang="ts">
	import { beatFractionOf, beatOf, isFresh, playback, projectCycle } from './playback.svelte';
	import { session } from './session.svelte';

	const live = $derived(session.engine?.engine === 'superdirt' && session.engine.status === 'live');

	$effect(() => {
		if (!live) return;
		let frame = 0;
		const tick = () => {
			const now = performance.now();
			const fresh = isFresh(now);
			if (fresh) {
				playback.cycle = projectCycle(now);
				playback.beat = beatOf(playback.cycle);
				playback.beatFraction = beatFractionOf(playback.cycle);
			}
			playback.fresh = fresh;
			frame = requestAnimationFrame(tick);
		};
		frame = requestAnimationFrame(tick);
		return () => cancelAnimationFrame(frame);
	});
</script>

{#if live}
	<div class="metronome" class:stale={!playback.fresh} aria-hidden="true">
		<div class="beats">
			{#each [0, 1, 2, 3] as beat (beat)}
				<span
					class="beat"
					class:on={playback.fresh && playback.beat === beat}
					style:opacity={playback.fresh && playback.beat === beat
						? 0.45 + 0.55 * (1 - playback.beatFraction)
						: 0.22}
				></span>
			{/each}
		</div>
		<div class="readout">
			<span class="cycle">{playback.cycle.toFixed(2)}</span>
			<span class="cps">{playback.cps > 0 ? playback.cps.toFixed(4) : '—'} cps</span>
		</div>
	</div>
{/if}

<style>
	.metronome {
		position: fixed;
		right: 0.75rem;
		bottom: 0.75rem;
		z-index: 6;
		display: flex;
		align-items: center;
		gap: 0.6rem;
		padding: 0.35rem 0.6rem;
		border: 1px solid color-mix(in srgb, var(--sw-pink) 30%, transparent);
		border-radius: 8px;
		background: color-mix(in srgb, #000 72%, transparent);
		font-family: Menlo, Monaco, Consolas, monospace;
		font-size: 0.68rem;
		pointer-events: none;
		transition: opacity 200ms ease-out;
	}

	.metronome.stale {
		opacity: 0.4;
	}

	.beats {
		display: flex;
		gap: 0.3rem;
	}

	.beat {
		width: 0.45rem;
		height: 0.45rem;
		border-radius: 50%;
		background: var(--sw-blue);
		opacity: 0.22;
	}

	.beat.on {
		box-shadow: 0 0 6px var(--sw-blue);
	}

	.readout {
		display: flex;
		flex-direction: column;
		gap: 0.02rem;
		line-height: 1.2;
	}

	.cycle {
		color: var(--sw-blue);
	}

	.cps {
		color: var(--sw-muted);
		font-size: 0.6rem;
	}
</style>
