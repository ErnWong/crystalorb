const puppeteer = require('puppeteer');
const path = require('path');
const util = require('util');
const fs = require('fs');
const UPNG = require('@pdf-lib/upng').default;

const WIDTH = 800;
const HEIGHT = 450;
const FRAMERATE = 60;

// We place the generated screenshot at a location that has a lower probability of being moved
// in the future, since README's image link will need to be absolute in order to work in docs.rs
const REPO_ROOT_DIR = path.resolve(__dirname, '..', '..', '..', '..');
const ASSETS_DIR = path.resolve(REPO_ROOT_DIR, 'assets');
const DESTINATION = path.resolve(ASSETS_DIR, 'screencapture.apng');

const keymap = new Map();

function add_keymap(id, left, right) {
  keymap.set(id, { left, right });
}

add_keymap('up', 'KeyW', 'ArrowUp');
add_keymap('down', 'KeyS', 'ArrowDown');
add_keymap('left', 'KeyA', 'ArrowLeft');
add_keymap('right', 'KeyD', 'ArrowRight');

const actions = new Map();

function act(seconds, side, action, param) {
  const frame = Math.floor(seconds * 60);
  if (!actions.has(frame)) {
    actions.set(frame, [])
  }
  actions.get(frame).push({ side, action, param });
}

act(1.0, "left", "connect");
act(4.0, "left", "on", "up");
act(4.4, "left", "off", "up");
act(4.5, "left", "on", "right");
act(5.0, "left", "on", "up");
act(5.4, "left", "off", "up");
act(5.5, "left", "off", "right");

act(3.0, "right", "connect");

act(7.0, "left", "on", "up");
act(7.4, "left", "off", "up");
act(7.5, "left", "on", "left");
act(8.0, "left", "on", "up");
act(8.4, "left", "off", "up");
act(8.5, "left", "off", "left");

act(7.0, "right", "on", "up");
act(7.4, "right", "off", "up");
act(7.5, "right", "on", "left");
act(8.0, "right", "on", "up");
act(8.4, "right", "off", "up");
act(8.5, "right", "off", "left");

(async () => {
  console.log('Launching browser');
  const browser = await puppeteer.launch();
  const page = await browser.newPage();
  await page.setViewport({ width: WIDTH, height: HEIGHT });
  await page.goto('http://localhost:4000/?screencapture', {
    waitUntil: 'networkidle0',
  });

  const frames = [];
  const delays = [];
  for (let frame = 0; frame < FRAMERATE * 10; frame++ ) {
    console.log('Capturing frame', frame);
    const actions_for_frame = actions.get(frame);
    if (actions_for_frame) {
      for (const action of actions_for_frame) {
        switch (action.action) {
          case "connect": {
            await page.click(`#connect-${action.side}`);
            break;
          }
          case "on": {
            await page.keyboard.down(keymap.get(action.param)[action.side]);
            break;
          }
          case "off": {
            await page.keyboard.up(keymap.get(action.param)[action.side]);
            break;
          }
        }
      }
    }
    await page.evaluate("window.next_screenshot_frame()");
    const frame_png = await page.screenshot({
      omitBackground: true,
    });
    const frame_img = UPNG.decode(frame_png);
    const frame_RGBA8 = UPNG.toRGBA8(frame_img)[0];
    frames.push(frame_RGBA8);
    delays.push(1 / FRAMERATE * 1000);
  }

  console.log('Closing browser');
  await browser.close();

  console.log(`Encoding APNG (${frames.length} frames)`);
  const apng = Buffer.from(UPNG.encode(frames, WIDTH, HEIGHT, 0, delays));

  console.log('Saving APNG to', DESTINATION);
  await util.promisify(fs.writeFile)(DESTINATION, apng);

  console.log('Done');

  console.warn('Remember to update README.markdown image link to point to the new commit hash');
})();
