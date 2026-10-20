// ============================================================================
// CENA SHINE — sol baixo com raios subindo
// ============================================================================
// Ideia: um "sol" perto do terço superior da tela. Os raios saem dele em
// coordenadas polares, são esticados na vertical e perdem força com a
// distância. A metade de baixo fica escura para a letra do Holyrics.
//
// Todo shader de cena começa em `fs_main`. Ele roda uma vez por pixel e
// recebe `frag`, que contém a posição daquele pixel na textura.
// ============================================================================

@fragment
fn fs_main(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    // Converte a posição do pixel para o sistema das cenas:
    // x em -1.6..+1.6, y em -1..+1, y positivo para cima.
    let p = screen_p(frag.xy);

    // `time` vai de 0 a quase 1. `loop_time` deforma o tempo de acordo com
    // a velocidade, mas mantém t=0 e t=1 iguais (loop sem emenda).
    let t = u.time;
    let phase = TAU * loop_time(t, u.params.x);

    // Semente e controles vindos dos sliders.
    let seed = u.seed;
    let intensity = u.params.z;
    let scale = u.params.y;

    // Centro do sol: x=0 (meio), y=0.42 (um pouco acima do centro).
    let center = vec2<f32>(0.0, 0.42);

    // `q` é a posição relativa ao sol. Multiplicar y por 0.55 "achata" o
    // espaço: o mesmo deslocamento vertical vale menos, então os raios
    // parecem mais compridos para cima/baixo.
    let q = (p - center) * vec2<f32>(1.0, 0.55) * scale;

    // `d` é a distância até o sol. `ang` é o ângulo em radianos, medido a
    // partir do eixo y (para cima). É o sistema polar: (d, ang) no lugar de
    // (x, y). Raios ficam naturais nesse sistema.
    let d = length(q);
    let ang = atan2(q.x, q.y);

    // "Wobble" deixa os raios orgânicos em vez de perfeitamente simétricos.
    // Duas camadas de ruído: uma larga e uma fina. As entradas usam
    // sin(phase)/cos(phase), que são periódicos, então o loop continua
    // fechando em t=0 e t=1.
    let wobble = (noise21(vec2<f32>(ang * 2.5 + seed, sin(phase) * 0.6 + seed)) - 0.5) * 0.8
        + (noise21(vec2<f32>(ang * 8.0 - seed, cos(phase) * 0.5 + seed)) - 0.5) * 0.25;

    // Fase angular dos raios. 22 = quantidade de raios ao redor do sol.
    // O `sin(phase)*0.25` faz o conjunto girar levemente para lá e para cá.
    let ray_phase = ang * 22.0 + wobble + sin(phase) * 0.25 + seed;

    // cos() devolve 1 no centro de cada raio e -1 entre eles.
    // max(0, ...) joga fora a parte negativa.
    // pow(..., 6) cria feixes largos; pow(..., 18) cria um miolo fino e
    // brilhante dentro de cada feixe. A soma dos dois dá volume.
    let base = max(0.0, cos(ray_phase));
    let shafts = pow(base, 6.0) * 0.55 + pow(base, 18.0) * 0.45;

    // Só queremos a luz subindo. q.y > 0 é acima do sol; a transição suave
    // evita um corte reto.
    let up_mask = smoothstep(-0.12, 0.25, q.y);

    // Falloff radial: exp(-d*k) cai suavemente com a distância.
    // Com k=1.6: d=0 -> 1; d=0.5 -> 0.45; d=1 -> 0.20; d=2 -> 0.04.
    // Ou seja, a luz some rápido, mas nunca vira zero de repente.
    let rays = shafts * exp(-d * 1.6) * up_mask;

    // Respiração: o brilho pulsa de leve, sem nunca ficar negativo.
    let breathe = 0.85 + 0.15 * sin(phase);

    // Halo: glow largo em volta do sol. O smoothstep corta a parte de baixo
    // para não clarear a região da letra.
    let halo = exp(-d * 6.5) * breathe * smoothstep(-0.45, 0.12, q.y);

    // Núcleo: gaussiana bem estreita. dot(q,q) = q.x² + q.y², então
    // exp(-(x²+y²)*80) vale 1 no centro, 0.45 a 0.1 de distância e
    // 0.02 a 0.22. Quanto maior o 80, menor e mais concentrado o ponto.
    let core = exp(-dot(q, q) * 80.0);

    // Brilho largo e de baixa frequência: 5 "pétalas" suaves que dão volume
    // ao fundo, em vez de deixar o céu totalmente preto.
    let broad = pow(max(0.0, cos(ang * 5.0 + seed)), 3.0)
        * exp(-d * 1.15)
        * smoothstep(-0.25, 0.3, q.y);

    // Flare horizontal: uma faixa fina no eixo do sol, como o reflexo de
    // uma lente. exp(-|y|*45) limita a altura; exp(-|x|*2.6) espalha na
    // horizontal.
    let flare = exp(-abs(q.y) * 45.0) * exp(-abs(q.x) * 2.6) * 0.2;

    // Cor do miolo: mistura a cor mais clara da paleta com branco puro para
    // o sol não ficar "amarelado" demais.
    let hot = mix(palette_linear(1.0), vec3<f32>(1.0), 0.35);

    // Poeira: divisão do espaço em células (floor). Só as células cujo hash
    // passa de 0.994 viram partículas. O deslocamento circular
    // (sin(phase), cos(phase)) faz a poeira flutuar e voltar ao mesmo lugar.
    let dust_p = p * 8.0 + vec2<f32>(seed, seed * 1.7) + vec2<f32>(sin(phase), cos(phase)) * 0.3;
    let dust_rnd = hash21(floor(dust_p));
    let dust = smoothstep(0.994, 1.0, dust_rnd) * exp(-length(fract(dust_p) - 0.5) * 6.0);

    // Composição: começa com um fundo quase preto, soma uma textura de
    // ruído bem sutil e depois vai empilhando as camadas de luz.
    var col = palette_linear(0.02) * 0.4;
    col += palette_linear(0.18) * fbm(p * 1.4 + vec2<f32>(seed, 0.0)) * 0.08;
    col += palette_linear(0.45) * broad * 0.06 * intensity;
    col += palette_linear(0.72) * rays * 1.15 * intensity;
    col += palette_linear(0.88) * halo * 0.55 * intensity;
    col += hot * core * 3.4 * intensity;
    col += palette_linear(0.9) * flare * intensity;
    col += palette_linear(0.62) * dust * 0.7 * intensity;

    // Vinheta + faixa de texto + tone mapping + sRGB.
    col = apply_masks(col, p);
    return finish(col);
}
