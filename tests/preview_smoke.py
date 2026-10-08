"""Browser-only tests of the offline design prototype. NOT native egui tests.

Install Python Playwright and a browser separately. Run:
  CHROMIUM=/usr/bin/chromium python tests/preview_smoke.py
"""
from __future__ import annotations
import json
import os
from pathlib import Path
import shutil
from playwright.sync_api import sync_playwright, expect

ROOT = Path(__file__).resolve().parents[1]
HTML = ROOT / 'preview' / 'hush-preview.html'
SHOTS = ROOT / 'preview' / 'screenshots'
SHOTS.mkdir(parents=True, exist_ok=True)
checks: list[str] = []

with sync_playwright() as p:
    executable = os.environ.get('CHROMIUM') or shutil.which('chromium')
    options = {'headless': True}
    if executable:
        options['executable_path'] = executable
    if os.name != 'nt':
        options['args'] = ['--no-sandbox']  # For this isolated test container only.
    browser = p.chromium.launch(**options)
    page = browser.new_page(viewport={'width': 1360, 'height': 960}, device_scale_factor=1)
    errors: list[str] = []
    requests: list[str] = []
    page.on('pageerror', lambda e: errors.append(str(e)))
    page.on('request', lambda r: requests.append(r.url))
    page.set_content(HTML.read_text(encoding='utf-8'), wait_until='load')

    def check(name: str, condition: bool) -> None:
        if not condition:
            raise AssertionError(name)
        checks.append(name)

    def capture(name: str) -> None:
        page.locator('.preview-info').hover()
        page.evaluate("document.querySelector('#toast').classList.remove('visible')")
        page.wait_for_timeout(400)  # Let theme/hover transitions finish, not a native frame test.
        page.screenshot(path=str(SHOTS / name), full_page=True)

    check('8 clearly marked fictitious inbox events', page.locator('[data-event]').count() == 8)
    expect(page.locator('.preview-info')).to_contain_text('kein nativer App-Screenshot')
    checks.append('prototype is visibly distinguished from a native screenshot')
    expect(page.locator('#status-label')).to_have_text('Demomodus')
    capture('inbox-dark.png')

    page.locator('[data-kind="request"]').click()
    check('PR-request filter selects 2 events', page.locator('[data-event]').count() == 2)
    page.locator('[data-kind="mention"]').click()
    check('mention filter selects 2 events', page.locator('[data-event]').count() == 2)
    page.locator('[data-page="inbox"]').click()
    page.locator('[data-tab="unread"]').click()
    check('unread filter selects 4 events', page.locator('[data-event]').count() == 4)
    page.locator('[data-tab="all"]').click()
    page.locator('#search').fill('a-phrase-not-present')
    expect(page.locator('.empty')).to_contain_text('Keine Treffer')
    checks.append('empty search has a real empty state')
    page.locator('#search').fill('atelier/web')
    check('repository search narrows the list', 0 < page.locator('[data-event]').count() < 8)
    page.locator('#search').fill('')
    page.keyboard.press('Control+k')
    expect(page.locator('#search')).to_be_focused()
    checks.append('keyboard search shortcut focuses the input')

    page.locator('[data-event="2"]').click()
    expect(page.locator('.detail')).to_be_visible()
    check('opening a detail marks it read', page.locator('#inbox-count').inner_text() == '3')
    capture('detail-dark.png')
    page.keyboard.press('Escape')
    check('Escape closes details', page.locator('.detail').count() == 0)
    page.locator('[data-event="2"]').click()
    page.locator('#archive-event').click()
    check('archive removes the selected event from inbox', page.locator('[data-event]').count() == 7)
    page.locator('[data-page="archive"]').click()
    check('archive contains the removed event', page.locator('[data-event]').count() == 1)
    page.locator('[data-event="2"]').click()
    page.locator('#archive-event').click()
    page.locator('[data-page="inbox"]').click()
    check('archive operation is reversible', page.locator('[data-event]').count() == 8)
    page.locator('#read-all').click()
    expect(page.locator('#inbox-count')).to_have_text('0')
    checks.append('read-all updates local counts')

    page.locator('#pause').click()
    expect(page.locator('.quiet-banner')).to_be_visible()
    checks.append('pause has an explicit visible state')
    page.locator('#pause').click()
    page.locator('#theme').click()
    check('theme switch enables the light palette', 'light' in page.locator('body').get_attribute('class'))
    capture('inbox-light.png')
    page.locator('#theme').click()

    page.locator('[data-page="settings"]').click()
    expect(page.locator('[data-switch="preview"]')).to_have_attribute('aria-checked', 'false')
    check('settings expose 4 event rules plus 2 notification switches', page.locator('[role="switch"]').count() == 6)
    capture('settings-dark.png')
    page.locator('[data-interval="300"]').click()
    expect(page.locator('[data-interval="300"]')).to_have_class('on')
    checks.append('poll interval is selectable')
    page.locator('#repos').fill('not-a-repository')
    page.locator('#save').click()
    expect(page.locator('#toast')).to_contain_text('organisation/repository')
    checks.append('invalid repository input is rejected')
    page.locator('#repos').fill('atelier/web\natlas/api')
    page.locator('#save').click()
    expect(page.locator('#toast')).to_contain_text('Keine echten Einstellungen gespeichert')
    checks.append('save remains explicitly demo-only')
    page.locator('#test-notification').click()
    expect(page.locator('.notification')).to_contain_text('SIMULIERTES SYSTEM-BANNER')
    expect(page.locator('.notification')).not_to_contain_text('atelier/desktop')
    checks.append('banner preview does not reveal private metadata by default')
    page.locator('.notification button').click()
    page.locator('[data-switch="preview"]').click()
    page.locator('#test-notification').click()
    expect(page.locator('.notification')).to_contain_text('atelier/desktop')
    checks.append('content preview requires the explicit switch')
    page.locator('.notification button').click()
    page.locator('[data-page="inbox"]').click()
    page.set_viewport_size({'width': 1000, 'height': 850})
    check('1000px viewport has no document-level horizontal overflow', page.evaluate('document.documentElement.scrollWidth <= window.innerWidth'))
    check('no JavaScript runtime errors', not errors)
    check('no network requests from the prototype', not requests)
    browser.close()

report = {'scope': 'HTML design prototype only; not native Rust/egui', 'passed': len(checks), 'checks': checks,
          'screenshots': ['preview/screenshots/' + n for n in ('inbox-dark.png','detail-dark.png','inbox-light.png','settings-dark.png')],
          'native_rust_compiled': False, 'native_os_notifications_tested': False, 'live_github_tested': False}
(ROOT / 'docs' / 'preview-test-results.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
print(json.dumps(report, ensure_ascii=False, indent=2))
