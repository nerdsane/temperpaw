"""Exercise the tenant-wide short-name boundary with real CSDL parsing."""

import unittest

from check_names import Names, check, names


def model(*schemas: str) -> bytes:
    return (
        '<Edmx xmlns="http://docs.oasis-open.org/odata/ns/edm">'
        + "".join(schemas)
        + "</Edmx>"
    ).encode()


class NamesTest(unittest.TestCase):
    def test_unrelated_live_duplicates_do_not_block_unique_new_names(self) -> None:
        live = names(
            model(
                '<EntityType Name="Agent"/><EntityType Name="Agent"/><EntitySet Name="Agents"/>'
            ),
            candidate=False,
        )
        check(Names(frozenset({"DsfThing"}), frozenset({"DsfThings"})), live)

    def test_ambiguous_live_name_cannot_be_claimed_by_previous_bundle(self) -> None:
        live = names(
            model(
                '<EntityType Name="DsfThing"/><EntityType Name="DsfThing"/><EntitySet Name="DsfThings"/>'
            ),
            candidate=False,
        )
        owned = Names(frozenset({"DsfThing"}), frozenset({"DsfThings"}))
        with self.assertRaisesRegex(ValueError, "ambiguous ownership"):
            check(owned, live, owned)

    def test_namespace_does_not_hide_type_collision(self) -> None:
        xml = model(
            '<Schema Namespace="A"><EntityType Name="DsfThing"/><EntitySet Name="DsfThings"/></Schema>',
            '<Schema Namespace="B"><EntityType Name="DsfThing"/></Schema>',
        )
        with self.assertRaisesRegex(ValueError, "Duplicate"):
            names(xml, candidate=True)

    def test_entity_sets_collide_across_containers(self) -> None:
        xml = model(
            '<EntityType Name="DsfThing"/><EntityContainer Name="A"><EntitySet Name="DsfThings"/></EntityContainer><EntityContainer Name="B"><EntitySet Name="DsfThings"/></EntityContainer>'
        )
        with self.assertRaisesRegex(ValueError, "Duplicate"):
            names(xml, candidate=True)

    def test_first_install_preserves_existing_dsf_deploy(self) -> None:
        check(
            Names(
                frozenset({"DsfRailwayServiceInstance"}),
                frozenset({"DsfRailwayServiceInstances"}),
            ),
            Names(frozenset({"DsfDeploy"}), frozenset({"DsfDeploys"})),
        )

    def test_existing_prefix_does_not_prove_ownership(self) -> None:
        same = Names(frozenset({"DsfDeploy"}), frozenset({"DsfDeploys"}))
        with self.assertRaisesRegex(ValueError, "owned outside"):
            check(same, same)

    def test_upgrade_can_only_reuse_previous_owned_names(self) -> None:
        own = Names(frozenset({"DsfThing"}), frozenset({"DsfThings"}))
        live = Names(own.types | {"DsfDeploy"}, own.sets | {"DsfDeploys"})
        check(own, live, own)
        with self.assertRaisesRegex(ValueError, "owned outside"):
            check(live, live, own)

    def test_app_cannot_redeclare_dependency(self) -> None:
        with self.assertRaisesRegex(ValueError, "unprefixed"):
            names(
                model('<EntityType Name="Effort"/><EntitySet Name="Efforts"/>'),
                candidate=True,
            )


if __name__ == "__main__":
    unittest.main()
