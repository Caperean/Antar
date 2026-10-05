import init, { Antar } from "./pkg/antar.js";

async function runGame() {
  await init();
  const canvas = document.getElementById("canvas");
  const ctx = await Antar.init(canvas); //

  // --- TWORZENIE TEKSTUR ---
  
  // Tekstura Gracza (zielony kwadrat 16x16)
  const playerPixels = new Uint8Array(16 * 16 * 4);
  for (let i = 0; i < playerPixels.length; i += 4) {
    playerPixels[i] = 46;     // R
    playerPixels[i + 1] = 204; // G
    playerPixels[i + 2] = 113; // B
    playerPixels[i + 3] = 255; // A
  }
  const playerTex = ctx.loadTexture(playerPixels, 16, 16); //

  // Tekstura Monety (żółty kwadrat 12x12)
  const coinPixels = new Uint8Array(12 * 12 * 4);
  for (let i = 0; i < coinPixels.length; i += 4) {
    coinPixels[i] = 241;    // R
    coinPixels[i + 1] = 196; // G
    coinPixels[i + 2] = 15;  // B
    coinPixels[i + 3] = 255; // A
  }
  const coinTex = ctx.loadTexture(coinPixels, 12, 12); //

  // --- STAN GRY ---

  const keys = {};
  window.addEventListener("keydown", (e) => (keys[e.code] = true));
  window.addEventListener("keyup", (e) => (keys[e.code] = false));

  let score = 0;
  let totalTime = 0; // Czas do animacji efatów

  const player = {
    x: 100,
    y: 300,
    w: 32,
    h: 32,
    vx: 0,
    vy: 0,
    speed: 300,
    jumpForce: -550,
    isGrounded: false,
    facingRight: true
  };

  const gravity = 1200;

  // Platformy
  const platforms = [
    { x: 0, y: 550, w: 800, h: 50 },    // Podłoże
    { x: 150, y: 420, w: 180, h: 20 },  // Platforma 1
    { x: 400, y: 320, w: 200, h: 20 },  // Platforma 2
    { x: 100, y: 200, w: 150, h: 20 }   // Platforma 3
  ];

  // Półprzezroczysta platforma testowa (użyje setGlobalAlpha)
  const ghostPlatform = { x: 300, y: 180, w: 120, h: 15 };

  // Monety
  const coins = [
    { x: 200, y: 380, w: 20, h: 20, collected: false },
    { x: 480, y: 280, w: 20, h: 20, collected: false },
    { x: 150, y: 160, w: 20, h: 20, collected: false }
  ];

  function checkAABB(rect1, rect2) {
    return (
      rect1.x < rect2.x + rect2.w &&
      rect1.x + rect1.w > rect2.x &&
      rect1.y < rect2.y + rect2.h &&
      rect1.y + rect1.h > rect2.y
    );
  }

  // --- GAME LOOP ---
  let lastTime = performance.now();

  function gameLoop(currentTime) {
    const dt = Math.min((currentTime - lastTime) / 1000, 0.1);
    lastTime = currentTime;
    totalTime += dt;

    // Ruch gracza
    player.vx = 0;
    if (keys["KeyA"] || keys["ArrowLeft"]) {
      player.vx = -player.speed;
      player.facingRight = false;
    }
    if (keys["KeyD"] || keys["ArrowRight"]) {
      player.vx = player.speed;
      player.facingRight = true;
    }

    if ((keys["Space"] || keys["ArrowUp"] || keys["KeyW"]) && player.isGrounded) {
      player.vy = player.jumpForce;
      player.isGrounded = false;
    }

    player.vy += gravity * dt;

    // Przemieszczenie X i kolizje (z uwzględnieniem ghost platformy)
    player.x += player.vx * dt;
    const allPlatforms = [...platforms, ghostPlatform];
    
    for (const plat of allPlatforms) {
      if (checkAABB(player, plat)) {
        if (player.vx > 0) player.x = plat.x - player.w;
        else if (player.vx < 0) player.x = plat.x + plat.w;
      }
    }

    // Przemieszczenie Y i kolizje
    player.y += player.vy * dt;
    player.isGrounded = false;

    for (const plat of allPlatforms) {
      if (checkAABB(player, plat)) {
        if (player.vy > 0) {
          player.y = plat.y - player.h;
          player.vy = 0;
          player.isGrounded = true;
        } else if (player.vy < 0) {
          player.y = plat.y + plat.h;
          player.vy = 0;
        }
      }
    }

    // Zbieranie monet
    for (const coin of coins) {
      if (!coin.collected && checkAABB(player, coin)) {
        coin.collected = true;
        score += 100;
        console.log(`Zbór monet! Punkty: ${score}`);
      }
    }

    // --- RENDEROWANIE ---
    try {
      ctx.clear("#1a1a2e"); //

      // 1. Rysowanie zwykłych platform
      for (const plat of platforms) {
        ctx.fillRect({ //
          rect: { x: plat.x, y: plat.y, w: plat.w, h: plat.h }, //
          color: { r: 0.3, g: 0.34, b: 0.49, a: 1.0 } //
        });
      }

      // TEST 1: Przezroczysta/Pulsująca platforma (używa save, setGlobalAlpha, restore)
      ctx.save(); //
      const alphaPulse = 0.4 + Math.sin(totalTime * 4) * 0.2; // Wartość 0.2 - 0.6
      ctx.setGlobalAlpha(alphaPulse); //
      ctx.fillRect({ //
        rect: { x: ghostPlatform.x, y: ghostPlatform.y, w: ghostPlatform.w, h: ghostPlatform.h }, //
        color: { r: 0.9, g: 0.3, b: 0.6, a: 1.0 } //
      });
      ctx.restore(); // Przywraca alpha = 1.0 dla reszty rysowania

      // TEST 2: Obracający się kryształ / przeszkoda dekoracyjna (używa rotation)
      ctx.drawImage({ //
        texture: coinTex, //
        src: { x: 0, y: 0, w: 12, h: 12 }, //
        dst: { x: 700, y: 150, w: 24, h: 24 }, //
        rotation: totalTime * 2, // Obrót w radianach
        color: { r: 0.2, g: 0.8, b: 1.0, a: 1.0 } // Niebieski tint
      });

      // 3. Rysowanie monet z dynamicznym tintem koloru
      for (const coin of coins) {
        if (!coin.collected) {
          ctx.drawImage({ //
            texture: coinTex, //
            src: { x: 0, y: 0, w: 12, h: 12 }, //
            dst: { x: coin.x, y: coin.y, w: coin.w, h: coin.h }, //
            color: { r: 1.0, g: 0.8 + Math.sin(totalTime * 8) * 0.2, b: 0.2, a: 1.0 } // Pulsowanie koloru
          });
        }
      }

      // WERYFIKACJA NAPRAWY BŁĘDU: Rysowanie gracza bezpośrednio przez flipX
      ctx.drawImage({ //
        texture: playerTex, //
        src: { x: 0, y: 0, w: 16, h: 16 }, //
        dst: { x: player.x, y: player.y, w: player.w, h: player.h }, //
        flipX: !player.facingRight, // ⚡ Użycie naprawionej flagi!
        color: { r: 1, g: 1, b: 1, a: 1 } //
      });

      ctx.present(); //
    } catch (e) {
      console.error("Błąd podczas renderowania klatki:", e); //
    }

    requestAnimationFrame(gameLoop);
  }

  requestAnimationFrame(gameLoop);
}

runGame().catch(console.error);