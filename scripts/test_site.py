import json
import re
import unittest
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[1]
SITE = ROOT / "site"


class Page(HTMLParser):
    def __init__(self, content):
        super().__init__()
        self.tags = []
        self.feed(content)

    def handle_starttag(self, tag, attrs):
        self.tags.append((tag, dict(attrs)))


class WebsiteTests(unittest.TestCase):
    def setUp(self):
        self.content = (SITE / "index.html").read_text()
        self.page = Page(self.content)

    def test_local_resources_and_anchors_exist(self):
        ids = {attrs["id"] for _, attrs in self.page.tags if "id" in attrs}
        for _, attrs in self.page.tags:
            for key in ("href", "src"):
                value = attrs.get(key, "")
                if not value or urlparse(value).scheme:
                    continue
                if value.startswith("#"):
                    self.assertIn(value[1:], ids)
                else:
                    self.assertTrue((SITE / value.lstrip("/")).is_file(), value)

    def test_install_command_matches_readme(self):
        readme = (ROOT / "README.md").read_text()
        command = re.search(r"curl -fsSL https://raw.githubusercontent.com/[^\n]+ \| sh", readme)[0]
        self.assertIn(command, self.content)
        self.assertIn("Editing is not yet available.", self.content)

    def test_production_metadata(self):
        metadata = {attrs.get("property", attrs.get("name")): attrs.get("content")
                    for tag, attrs in self.page.tags if tag == "meta"}
        self.assertEqual(metadata["og:url"], "https://gezi.arieszhou.com/")
        self.assertEqual(metadata["og:image"], "https://gezi.arieszhou.com/og-image.png")
        self.assertEqual(metadata["twitter:card"], "summary_large_image")
        self.assertTrue((SITE / "og-image.png").is_file())

    def test_no_exploration_or_local_paths(self):
        for text in ("design-a/", "design-b/", "Compare directions", "/private/tmp",
                     "/Users/", "localhost", "127.0.0.1"):
            self.assertNotIn(text, self.content)

    def test_static_hosting_contract(self):
        config = json.loads((ROOT / "vercel.json").read_text())
        self.assertEqual(config["outputDirectory"], "site")
        self.assertEqual(config["buildCommand"], "")
        self.assertEqual(config["installCommand"], "")

    def test_logo_is_official(self):
        self.assertEqual((SITE / "assets/logo.svg").read_bytes(),
                         (ROOT / "assets/brand/logo.svg").read_bytes())


if __name__ == "__main__":
    unittest.main()
