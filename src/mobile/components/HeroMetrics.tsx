import styles from "./HeroMetrics.module.css";

interface HeroMetricsProps {
	workingCount: number;
	idleCount: number;
	awaitingCount: number;
	onAwaitingClick?: () => void;
}

export function HeroMetrics(props: HeroMetricsProps) {
	return (
		<div class={styles.row}>
			<div class={styles.card} data-testid="metric-card">
				<span class={styles.number} data-testid="metric-number">
					{props.workingCount}
				</span>
				<span class={styles.label} data-testid="metric-label">
					Working
				</span>
			</div>
			<div class={styles.card} data-testid="metric-card">
				<span class={styles.number} data-testid="metric-number">
					{props.idleCount}
				</span>
				<span class={styles.label} data-testid="metric-label">
					Idle
				</span>
			</div>
			<button
				type="button"
				class={`${styles.card} ${styles.awaiting}`}
				data-testid="metric-card"
				disabled={props.awaitingCount === 0}
				onClick={props.onAwaitingClick}
			>
				<span class={styles.number} data-testid="metric-number">
					{props.awaitingCount}
				</span>
				<span class={styles.label} data-testid="metric-label">
					Awaiting
				</span>
			</button>
		</div>
	);
}
