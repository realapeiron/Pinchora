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


class CosmeticsTests(unittest.TestCase):
    BACKGROUNDS = json.loads(Path(__file__).with_name("backgrounds.json").read_text(encoding="utf-8"))
    BADGES = json.loads(Path(__file__).with_name("badges.json").read_text(encoding="utf-8"))

    TITLES = json.loads(Path(__file__).with_name("titles.json").read_text(encoding="utf-8"))

    def errors(self, backgrounds=None, badges=None, titles=None):
        check.ERRORS.clear()
        if backgrounds is not None:
            check.check_backgrounds(backgrounds)
        if badges is not None:
            check.check_badges(badges, 72)
        if titles is not None:
            records = check.entries(self.BADGES, "badges.json", 0)
            check.check_titles(titles, records, 72)
        return check.ERRORS[:]

    def test_shipped_files_are_valid(self):
        self.assertEqual(self.errors(self.BACKGROUNDS, self.BADGES, self.TITLES), [])

    def test_background_accent_and_price(self):
        for change in ({"accent": "8eddd1"}, {"accent": "#8EDDD1"}, {"price": 0}, {"price": 800.0}, {"price": 99999}):
            with self.subTest(change=change):
                broken = copy.deepcopy(self.BACKGROUNDS)
                broken[0].update(change)
                self.assertTrue(self.errors(backgrounds=broken))

    def test_badge_fields_follow_kind(self):
        shop = next(i for i, b in enumerate(self.BADGES) if b["kind"] == "shop")
        for index, change in ((0, {"price": 10}), (0, {"stat": "xp"}), (0, {"at": 0}), (shop, {"stat": "level"}),
                              (shop, {"price": -1}), (0, {"kind": "gift"})):
            with self.subTest(index=index, change=change):
                broken = copy.deepcopy(self.BADGES)
                broken[index].update(change)
                self.assertTrue(self.errors(badges=broken))

    def test_fish_badges_cannot_ask_for_more_species_than_exist(self):
        broken = copy.deepcopy(self.BADGES)
        next(b for b in broken if b.get("stat") == "fish_species")["at"] = 73
        self.assertTrue(self.errors(badges=broken))

    def test_badge_rarity_is_required_and_follows_difficulty(self):
        shop = next(i for i, b in enumerate(self.BADGES) if b["kind"] == "shop" and b["price"] == 1000)
        for index, change in ((0, {"rarity": "mythic"}), (0, {"rarity": None}),
                              (shop, {"rarity": "legendary"}),  # a cheap badge cannot be legendary
                              (0, {"rarity": "legendary"})):  # nor can the first level badge outrank the next
            with self.subTest(index=index, change=change):
                broken = copy.deepcopy(self.BADGES)
                broken[index].update(change)
                self.assertTrue(self.errors(badges=broken))
        broken = copy.deepcopy(self.BADGES)
        del broken[0]["rarity"]
        self.assertTrue(self.errors(badges=broken))

    def test_titles_follow_their_kind(self):
        badge = next(i for i, t in enumerate(self.TITLES) if t["kind"] == "badge")
        stat = next(i for i, t in enumerate(self.TITLES) if t["kind"] == "stat")
        special = next(i for i, t in enumerate(self.TITLES) if t["kind"] == "special")
        for index, change in ((badge, {"badge": "no_such_badge"}), (stat, {"at": 0}), (stat, {"stat": "xp"}),
                              (stat, {"badge": "crab_cap"}), (special, {"at": 5}), (stat, {"kind": "gift"}),
                              (stat, {"name": "A title that is far too long to fit"}),
                              (special, {"kind": "stat", "stat": "level", "at": 99})):
            with self.subTest(index=index, change=change):
                broken = copy.deepcopy(self.TITLES)
                broken[index].update(change)
                self.assertTrue(self.errors(titles=broken))

    def test_titles_need_the_house_of_the_week_and_distinct_thresholds(self):
        broken = [t for t in self.TITLES if t["id"] != "house_of_the_week"]
        self.assertTrue(any("house_of_the_week" in e for e in self.errors(titles=broken)))
        broken = copy.deepcopy(self.TITLES)
        levels = [t for t in broken if t.get("stat") == "level"]
        levels[1]["at"] = levels[0]["at"]
        self.assertTrue(any("different thresholds" in e for e in self.errors(titles=broken)))


class GameContentTests(unittest.TestCase):
    TRIVIA = json.loads(Path(__file__).with_name("trivia.json").read_text(encoding="utf-8"))
    HANGMAN = json.loads(Path(__file__).with_name("hangman.json").read_text(encoding="utf-8"))
    WORDS = json.loads(Path(__file__).with_name("words.json").read_text(encoding="utf-8"))

    def errors(self, function, raw):
        check.ERRORS.clear()
        function(raw)
        return check.ERRORS[:]

    def test_shipped_game_content_is_clean(self):
        self.assertEqual(self.errors(check.check_trivia, self.TRIVIA), [])
        self.assertEqual(self.errors(check.check_hangman, self.HANGMAN), [])
        self.assertEqual(self.errors(check.check_words, self.WORDS), [])

    def test_trivia_answers_must_be_one_of_the_options(self):
        for change in ({"answer": "Not an option"}, {"options": ["A", "B", "C"]}, {"options": ["A", "A", "B", "C"]},
                       {"category": "sports"}, {"difficulty": "brutal"}):
            with self.subTest(change=change):
                broken = copy.deepcopy(self.TRIVIA)
                broken[0].update(change)
                self.assertTrue(any("trivia.json[0]" in e for e in self.errors(check.check_trivia, broken)))

    def test_trivia_questions_and_ids_must_be_unique(self):
        broken = copy.deepcopy(self.TRIVIA)
        broken[1]["question"] = broken[0]["question"].upper()
        self.assertTrue(any("duplicate question" in e for e in self.errors(check.check_trivia, broken)))
        broken = copy.deepcopy(self.TRIVIA)
        broken[1]["id"] = broken[0]["id"]
        self.assertTrue(any("duplicate id" in e for e in self.errors(check.check_trivia, broken)))

    def test_hangman_words_are_plain_and_unique(self):
        for word in ("Sea Horse", "sea  horse", "sea-horse", "a b c", "abc", "x" * 17, "sea horse boat"):
            with self.subTest(word=word):
                broken = copy.deepcopy(self.HANGMAN)
                broken[0]["word"] = word
                self.assertTrue(any("hangman.json[0]" in e for e in self.errors(check.check_hangman, broken)))
        broken = copy.deepcopy(self.HANGMAN)
        broken[1]["word"] = broken[0]["word"]
        self.assertTrue(any("duplicate word" in e for e in self.errors(check.check_hangman, broken)))

    def test_word_lists_are_sorted_five_letter_words(self):
        for key, word in (("answers", "toolong"), ("answers", "Abcde"), ("allowed", "abc"), ("allowed", "bitch")):
            with self.subTest(key=key, word=word):
                broken = copy.deepcopy(self.WORDS)
                broken[key].append(word)
                self.assertTrue(self.errors(check.check_words, broken))
        broken = copy.deepcopy(self.WORDS)
        broken["allowed"].remove(broken["answers"][0])
        self.assertTrue(any("not allowed guesses" in e for e in self.errors(check.check_words, broken)))


if __name__ == "__main__":
    unittest.main()
