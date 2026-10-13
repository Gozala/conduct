"""The demo in a browser: TodoMVC's flows, and the shelf's editor.

Serve demo/dist (built by demo/build.sh) and run this with the site's URL:

    python3 -m http.server 8087 --directory demo/dist &
    python3 demo/tests/browser.py http://127.0.0.1:8087/

It needs Playwright's Python package and its Chromium. Each test starts in
a new browser context, so with a new service worker and an empty IndexedDB.
"""

import asyncio
import pathlib
import sys
import time

from playwright.async_api import async_playwright

URL = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8087/"
EXAMPLES = pathlib.Path(__file__).resolve().parents[2] / "examples"

# The titles and completion of the todos <todo-app> shows.
ITEMS = """Array.from(document.querySelector('todo-app').shadowRoot.querySelectorAll('todo-item'))
  .map(e => (e.getAttribute('title') || '') + '|' + e.getAttribute('completed'))"""

# The class of the element that has focus, through shadow roots.
FOCUSED = """(() => { let a = document.activeElement;
  while (a && a.shadowRoot && a.shadowRoot.activeElement) a = a.shadowRoot.activeElement;
  return a && a.className })()"""

ITEM_SECTION = "#shelf section[data-name=todo-item]"


async def wait_for(page, js, timeout=20):
    end = time.time() + timeout
    while time.time() < end:
        value = await page.evaluate(js)
        if value:
            return value
        await asyncio.sleep(0.1)
    raise AssertionError("timed out waiting for: " + js)


async def items(page):
    return await page.evaluate(ITEMS)


async def open_page(browser, logs):
    context = await browser.new_context(viewport={"width": 1280, "height": 900})
    page = await context.new_page()
    page.on("console", lambda m: logs.append(f"[{m.type}] {m.text}"))
    page.on("pageerror", lambda e: logs.append(f"[pageerror] {e}"))
    started = time.time()
    await page.goto(URL)
    await page.locator("input.new-todo").wait_for(timeout=60000)
    print(f"  interactive after {time.time() - started:.2f} s")
    return page


async def todomvc(browser, logs):
    page = await open_page(browser, logs)
    field = page.locator("input.new-todo")
    # Typed at once, with no keystrokes between: a render for the first
    # add must neither clear nor replace the field.
    for title in ["buy milk", "write conduct", "walk"]:
        await field.fill(title)
        await field.press("Enter")
    await wait_for(page, f"({ITEMS}).length === 3")
    assert await items(page) == ["buy milk|false", "write conduct|false", "walk|false"], await items(page)
    assert await page.evaluate(FOCUSED) == "new-todo", await page.evaluate(FOCUSED)

    await page.locator("todo-item").nth(1).locator("input.toggle").click()
    await wait_for(page, f"({ITEMS})[1] === 'write conduct|true'")
    footer = await page.evaluate(
        "document.querySelector('todo-app').shadowRoot.querySelector('todo-footer').shadowRoot.textContent"
    )
    assert "2 items left" in footer, footer

    await page.locator("todo-footer a[href='#/active']").click()
    await wait_for(page, f"({ITEMS}).length === 2")
    await page.locator("todo-footer a[href='#/completed']").click()
    await wait_for(page, f"({ITEMS}).length === 1")
    await page.locator("todo-footer a[href='#/']").click()
    await wait_for(page, f"({ITEMS}).length === 3")

    await page.locator("todo-item").nth(0).locator("label").dblclick()
    edit = page.locator("todo-item").nth(0).locator("input.edit")
    await edit.fill("buy oat milk")
    await edit.press("Enter")
    await wait_for(page, f"({ITEMS})[0] === 'buy oat milk|false'")

    await page.locator("todo-item").nth(2).hover()
    await page.locator("todo-item").nth(2).locator("button.destroy").click()
    await wait_for(page, f"({ITEMS}).length === 2")
    await page.locator("todo-footer button.clear-completed").click()
    await wait_for(page, f"({ITEMS}).length === 1")
    await page.locator("todo-input .toggle-all").click()
    await wait_for(page, f"({ITEMS})[0] === 'buy oat milk|true'")

    await page.reload()
    await page.locator("input.new-todo").wait_for(timeout=60000)
    await wait_for(page, f"({ITEMS}).length === 1")
    assert await items(page) == ["buy oat milk|true"], await items(page)
    await page.context.close()


async def rest_on(page, section, before, word):
    """Scroll the editor of `section` to `word`, which `before` precedes,
    rest the pointer on it, and answer the tooltip's text."""
    point = await page.evaluate(
        """([section, before, word]) => {
          const sec = document.querySelector(section);
          const text = sec.querySelector('textarea');
          const at = text.value.indexOf(before + word) + before.length;
          const line = text.value.slice(0, at).split('\\n').length - 1;
          text.scrollTop = Math.max(0, line * parseFloat(getComputedStyle(text).lineHeight) - 40);
          text.dispatchEvent(new Event('scroll'));
          const walker = document.createTreeWalker(sec.querySelector('pre.highlight'), NodeFilter.SHOW_TEXT);
          let node, pos = 0;
          while ((node = walker.nextNode())) {
            if (pos + node.length > at) {
              const range = document.createRange();
              range.setStart(node, at - pos);
              range.setEnd(node, at - pos + word.length);
              const box = range.getBoundingClientRect();
              return [box.x + box.width / 2, box.y + box.height / 2];
            }
            pos += node.length;
          }
          return null;
        }""",
        [section, before, word],
    )
    await page.mouse.move(point[0], point[1])
    await asyncio.sleep(0.1)
    await page.mouse.move(point[0] + 1, point[1])
    return await wait_for(
        page,
        f"(() => {{ const t = document.querySelector('{section} .tooltip'); return t && !t.hidden && t.textContent }})()",
    )


async def shelf(browser, logs):
    page = await open_page(browser, logs)
    await page.locator("#shelf-toggle").click()
    await page.get_by_role("tab", name="<todo-item>").click()

    # The type the compiler infers for a definition, where it is named.
    shown = await rest_on(page, ITEM_SECTION, "(define (", "view")
    assert shown == "view : ((\\ {done: boolean, editing: boolean, title: text}) : view)", shown

    # A type error, listed with its line and column.
    await page.locator(f"{ITEM_SECTION} textarea").click()
    await page.keyboard.press("Control+End")
    await page.keyboard.type('\n(define (oops x) (+ x "one"))')
    problems = await wait_for(
        page,
        f"(() => {{ const p = document.querySelector('{ITEM_SECTION} .problems'); return p && !p.hidden && p.textContent }})()",
    )
    assert "in `oops`: type error: argument 2 of `+`: text is not an integer" in problems, problems

    # A change that compiles: a save compiles the element again.
    shipped = (EXAMPLES / "todo-item.cdt").read_text()
    await page.evaluate(
        """([selector, text]) => { const area = document.querySelector(selector);
             area.value = text;
             area.dispatchEvent(new InputEvent('input', {data: 'x'})); }""",
        [ITEM_SECTION + " textarea", shipped.replace('"Delete the todo"', '"Remove the todo"')],
    )
    await page.locator(f"{ITEM_SECTION} button[data-action='save']").click()
    await page.locator("input.new-todo").fill("check the label")
    await page.locator("input.new-todo").press("Enter")
    await wait_for(
        page,
        """(() => { const buttons = Array.from(document.querySelector('todo-app').shadowRoot.querySelectorAll('todo-item'))
             .map(i => i.shadowRoot.querySelector('button.destroy')).filter(Boolean);
           return buttons.length && buttons.every(b => b.getAttribute('aria-label') === 'Remove the todo') })()""",
    )
    await page.context.close()


async def main():
    failed = False
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch()
        for test in [todomvc, shelf]:
            logs = []
            print(f"{test.__name__}:")
            try:
                await test(browser, logs)
                print("  ok")
            except Exception as error:  # noqa: BLE001 - report each test
                failed = True
                print(f"  FAILED: {error}")
                print("\n".join("    " + line for line in logs[-30:]))
        await browser.close()
    sys.exit(1 if failed else 0)


asyncio.run(main())
