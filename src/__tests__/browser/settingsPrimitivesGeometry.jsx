import { render } from 'solid-js/web';
import { Button } from '../../components/ui/Button';
import { DangerZone, EmptyState, ListRow, SectionCard, SettingPageHeader, StatusSummary } from '../../components/SettingsPanel/SettingsPrimitives';
import '../../global.css';

// Actual presentation slots and shared targets; no persistence or runtime discovery.
render(() => <section id="geometry" style="width:min(900px,100%);padding:16px;background:var(--bg-secondary)">
  <SettingPageHeader title="Settings actions" actions={<Button>Page action</Button>} />
  <SectionCard title="Card actions" headingActions={<Button>Heading action</Button>} actions={<Button>Footer action</Button>}>
    <ListRow name="Existing row" actions={<Button>Row action</Button>} />
  </SectionCard>
  <StatusSummary label="Not checked" actions={<Button>Status action</Button>} />
  <EmptyState title="No entries" description="Caller-owned empty state" action={<Button>Add entry</Button>} />
  <DangerZone title="Remove entry" scope="Fixture only" consequence="No mutation runs.">
    <Button variant="danger">Review removal</Button>
    <Button disabled>Unavailable action</Button>
  </DangerZone>
</section>, document.getElementById('root'));
window.controlGeometryReady = true;
