"""Check every declared Effort producer against its strict action parameters."""
from pathlib import Path
import tomllib


def main() -> None:
    apps = Path(__file__).resolve().parents[3]
    effort = tomllib.loads((apps / 'paw-patrol/specs/effort.ioa.toml').read_text())
    parameters = {
        action['name']: {item if isinstance(item, str) else item['name'] for item in action.get('params', [])}
        for action in effort['action']
    }
    checked = 0
    for path in sorted(apps.glob('*/specs/*.ioa.toml')):
        for action in tomllib.loads(path.read_text()).get('action', []):
            for trigger in action.get('triggers', []):
                if trigger.get('target_entity') != 'Effort':
                    continue
                target = trigger['target_action']
                incoming = set(trigger.get('params_from', {})) | set(trigger.get('params', {}))
                if target not in parameters or incoming - parameters[target]:
                    raise SystemExit(f'{path.name} {action["name"]}: undeclared Effort.{target} parameters {sorted(incoming)}')
                checked += 1
    if checked < 7:
        raise SystemExit('Expected Intent and both existing deployment producers')
    print(f'{checked} declared Effort producers match the strict contract')


if __name__ == '__main__':
    main()
