#!/usr/bin/env python3
"""Regression checks for isometric placement validation (standard library only)."""

import copy
import json
import unittest
from pathlib import Path

import check


CATALOG = json.loads(Path(__file__).with_name("furniture.json").read_text(encoding="utf-8"))
BY_ID = {item["id"]: item for item in CATALOG}


class IsometricPlacementTests(unittest.TestCase):
    def errors(self, item_id, changes=None, remove=()):
        item = copy.deepcopy(BY_ID[item_id])
        item.update(changes or {})
        for key in remove:
            item.pop(key, None)
        check.ERRORS.clear()
        check.placement(item, item_id)
        return check.ERRORS[:]

    def test_valid_examples_cover_all_layers_and_mounting(self):
        for item_id in ("hemloop_chair", "coilcap_desk_lamp", "memonest_desk_calendar",
                        "leafledger_wall_shelf", "hemcheck_rug", "gatherbud_platform"):
            with self.subTest(item_id=item_id):
                self.assertEqual(self.errors(item_id), [])

    def test_floor_and_surface_width_is_required(self):
        for item_id in ("hemloop_chair", "coilcap_desk_lamp"):
            with self.subTest(item_id=item_id):
                self.assertTrue(any("iso_width" in error for error in self.errors(item_id, remove=("iso_width",))))

    def test_width_rejects_nonintegers_and_out_of_bounds(self):
        for value in (None, True, 32.0, "32", 0, 11, 65):
            with self.subTest(value=value):
                self.assertTrue(self.errors("hemloop_chair", {"iso_width": value}))

    def test_width_boundaries_for_one_tile_floor_item(self):
        for value in (12, 64):
            with self.subTest(value=value):
                self.assertEqual(self.errors("hemloop_chair", {"iso_width": value}), [])

    def test_surface_sprite_has_small_decor_scale(self):
        for value in (16, 36):
            with self.subTest(value=value):
                self.assertEqual(self.errors("coilcap_desk_lamp", {"iso_width": value}), [])
        for value in (15, 37):
            with self.subTest(value=value):
                self.assertTrue(self.errors("coilcap_desk_lamp", {"iso_width": value}))

    def test_wall_and_rug_must_not_have_iso_width(self):
        for item_id in ("foliohour_clock", "hemcheck_rug"):
            with self.subTest(item_id=item_id):
                self.assertTrue(self.errors(item_id, {"iso_width": 32}))

    def test_support_height_required_for_floor_and_wall_supports(self):
        for item_id in ("crumbshoal_coffee_table", "leafledger_wall_shelf"):
            with self.subTest(item_id=item_id):
                self.assertTrue(self.errors(item_id, remove=("surface_height",)))
                for value in (None, False, 30.0, "30", 9, 71):
                    self.assertTrue(self.errors(item_id, {"surface_height": value}))
                for value in (10, 70):
                    self.assertEqual(self.errors(item_id, {"surface_height": value}), [])

    def test_decor_does_not_gain_support_height(self):
        self.assertTrue(self.errors("coilcap_desk_lamp", {"surface_height": 30}))

    def test_brief_prefixes_match_layer(self):
        for item_id in ("hemloop_chair", "coilcap_desk_lamp", "foliohour_clock", "hemcheck_rug"):
            with self.subTest(item_id=item_id):
                self.assertTrue(self.errors(item_id, {"sprite_brief": "Top-down 3/4: coral wood."}))

    def test_isometric_orientation_is_explicit(self):
        for text in ("Isometric: coral chair.", "Isometric: 2:1, front facing upper-right; coral chair."):
            with self.subTest(text=text):
                self.assertTrue(self.errors("hemloop_chair", {"sprite_brief": text}))

    def test_recolourable_items_keep_coral(self):
        self.assertTrue(self.errors("pebblerest_stool", {
            "sprite_brief": "Isometric: 2:1, front facing lower-left; navy wooden stool.",
        }))

    def test_rug_requires_full_rectangle(self):
        self.assertTrue(self.errors("hemcheck_rug", {"sprite_brief": "Top-down flat: Round coral wool rug."}))

    def test_full_furniture_width_cannot_shrink(self):
        self.assertTrue(self.errors("crumbshoal_coffee_table", {"iso_width": 48}))
        self.assertEqual(self.errors("crumbshoal_coffee_table"), [])

    def test_expanded_footprint_bounds(self):
        self.assertEqual(self.errors("gatherbud_platform"), [])  # 4x4 floor
        self.assertEqual(self.errors("quiltshoal_rug"), [])  # 4x4 rug
        self.assertEqual(self.errors("cloudindex_mural"), [])  # 4x3 wall
        self.assertEqual(self.errors("tucktide_mat"), [])  # 1x1 rug
        for item_id, footprint in (("gatherbud_platform", [4, 5]), ("cloudindex_mural", [4, 4]),
                                   ("coilcap_desk_lamp", [1, 2]), ("hemloop_chair", [True, 1])):
            with self.subTest(item_id=item_id, footprint=footprint):
                self.assertTrue(self.errors(item_id, {"footprint": footprint}))


if __name__ == "__main__":
    unittest.main()
