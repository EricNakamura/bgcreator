// ============================================================================
// CENA GRADIENT — seis estilos de gradiente procedural
// ============================================================================
// Todos os estilos recebem os mesmos argumentos:
//   p         = posição do pixel em coordenadas de tela (-1.6..1.6, -1..1)
//   phase     = tempo do loop em radianos (0..TAU)
//   seed      = semente para variar o desenho
//   scale     = escala do padrão (slider)
//   intensity = brilho geral (slider)
//
// Cada função devolve uma cor em espaço LINEAR. No fim de fs_main passamos
// por apply_masks (vinheta/faixa de texto) e finish (tone map + sRGB).
// O estilo ativo vem de u.params2.y (0..5), escolhido na UI.
// ============================================================================

// ----------------------------------------------------------------------------
// HORIZONTE — céu, chão e um sol baixo
// ----------------------------------------------------------------------------
// A ideia é montar um "nascer do sol": metade de baixo escura, uma faixa
// quente no horizonte e o céu escurecendo para o topo.
fn horizon_col(
    p: vec2<f32>,
    phase: f32,
    seed: f32,
    scale: f32,
    intensity: f32,
) -> vec3<f32> {
    // Estica um pouco o eixo y e aplica a escala do usuário.
    let q = p * vec2<f32>(1.0, 1.1) * scale;

    // Drift é um deslocamento circular periódico. Ele mexe no ruído sem
    // criar um salto no fim do loop.
    let drift = vec2<f32>(sin(phase) * 0.06, cos(phase) * 0.04);

    // Ruído suave que deforma a linha do horizonte.
    let warp = fbm(q * 1.15 + vec2<f32>(seed, seed * 0.7) + drift);

    // v transforma y (-1..1) em 0..1: 0 = base da tela, 1 = topo.
    let v = q.y * 0.5 + 0.5;

    // Duas ondas de frequências diferentes para o horizonte não ficar reto.
    // A fase entra com sinal oposto na segunda para o movimento parecer
    // orgânico, mas continua periódica.
    let wave = 0.005 * sin(q.x * 1.4 + phase + seed)
        + 0.035 * sin(q.x * 2.8 - phase + seed * 1.7);

    // Altura do horizonte: 0.60 fica acima do centro, deixando a parte de
    // baixo livre para a letra. O warp dá a irregularidade.
    let h = 0.40 + wave + (warp - 0.5) * 0.12;

    // `below` é 0 no fundo e sobe até 1 perto do horizonte.
    // `above` é 0 no horizonte e sobe até 1 no topo.
    let below = smoothstep(-0.1, h + 0.1, v);
    let above = smoothstep(h - 0.05, 1.1, v);

    // Chão: do quase preto para a cor média da paleta.
    // Céu: da cor média para o quase preto no topo.
    let lower = mix(palette_linear(0.02), palette_linear(0.34), below);
    let upper = mix(palette_linear(0.22), palette_linear(0.04), above);

    // Mistura os dois lados com uma transição suave em volta de h.
    let blend = smoothstep(h - 0.14, h + 0.14, v);
    var col = mix(lower, upper, blend);

    // Banda luminosa exatamente no horizonte: gaussiana estreita em v.
    // exp(-|v-h|*22) vale 1 no horizonte e cai para ~0.11 a 0.1 de distância.
    let band = exp(-abs(v - h) * 22.0);
    col += palette_linear(0.78) * band * 0.55 * intensity;

    // Sol: distância até um ponto um pouco acima do horizonte. Multiplicar
    // a componente vertical por 1.8 deixa o sol mais oval que um círculo.
    let sun_d = length(vec2<f32>(q.x, (v - h - 0.06) * 1.8));
    let sun = exp(-sun_d * sun_d * 26.0);
    col += palette_linear(1.0) * sun * 0.8 * intensity;

    // Lavagem de cor com o ruído: tira a cara de "gradiente matemático".
    let wash = smoothstep(0.0, 1.0, warp) * 0.1;
    col = mix(col, palette_linear(0.55), wash);
    return col;
}

// ----------------------------------------------------------------------------
// AURORA — fitas de luz no céu
// ----------------------------------------------------------------------------
fn aurora_col(
    p: vec2<f32>,
    phase: f32,
    seed: f32,
    scale: f32,
    intensity: f32,
) -> vec3<f32> {
    // y levemente comprimido para as fitas ficarem mais largas.
    let q = p * vec2<f32>(1.0, 0.9) * scale;

    // Fundo quase preto.
    var col = palette_linear(0.02) * 0.4;

    // Estrelas: o espaço é dividido em células (q*11). Só células cujo hash
    // passa de 0.995 acendem. O ponto dentro da célula é suavizado por uma
    // gaussiana. Como não depende de phase, as estrelas ficam paradas.
    let star_p = q * 11.0 + vec2<f32>(seed, seed * 0.3);
    let star = smoothstep(0.995, 1.0, hash21(floor(star_p)))
        * exp(-length(fract(star_p) - 0.5) * 7.0);
    col += palette_linear(0.95) * star * 0.45;

    // Três fitas. O loop vai de 0 a 2; `fi` é o índice como float.
    for (var i = 0u; i < 3u; i += 1u) {
        let fi = f32(i);

        // Altura central da fita: sobe com o índice e oscila em x.
        // `phase * (1.0 + fi)` usa múltiplos inteiros de uma volta, então
        // em t=1 a fase é a mesma de t=0 (loop fechado).
        let y0 = 0.16 + 0.18 * fi
            + 0.10 * sin(q.x * (1.0 + 0.35 * fi) + phase * (1.0 + fi) + seed + fi * 2.1);

        // Fitas de cima são um pouco mais largas.
        let width = 0.075 + 0.02 * fi;

        // Gaussiana: 1 no centro da fita, 0 longe dela. É o que dá o
        // aspecto de "cortina" suave, sem bordas duras. Se u=(q.y-y0)/width,
        // então u=0 -> 1; u=1 -> 0.37; u=2 -> 0.018. Por isso `width`
        // controla a largura visível da fita.
        let ribbon = exp(-pow((q.y - y0) / width, 2.0));

        // Variação de brilho ao longo da fita. Também é periódica.
        let flow = 0.55 + 0.45 * sin(q.x * 2.4 + phase * (1.0 + fi) + fi * 1.7 + seed);

        // Cada fita puxa uma cor um pouco mais clara da paleta.
        let color = palette_linear(0.45 + 0.16 * fi);
        col += color * ribbon * flow * 0.4 * intensity;
    }

    // Brilho fraco na base, como se a aurora iluminasse o horizonte.
    col += palette_linear(0.7) * exp(-abs(q.y + 0.55) * 6.0) * 0.12;
    return col;
}

// ----------------------------------------------------------------------------
// MESH — manchas de cor misturadas
// ----------------------------------------------------------------------------
// É uma média ponderada de várias "bolhas" coloridas. Em vez de misturar
// em RGB (que passa por cinza), acumulamos em OKLab, que preserva a
// vivacidade das cores.
fn mesh_col(
    p: vec2<f32>,
    phase: f32,
    seed: f32,
    scale: f32,
    intensity: f32,
) -> vec3<f32> {
    // Domain warp: desloca o espaço com senoides antes de calcular as
    // bolhas. Isso deixa as formas fluidas em vez de círculos perfeitos.
    var q = p * scale;
    q += vec2<f32>(sin(q.y * 2.2 + phase) * 0.10, cos(q.x * 1.8 - phase) * 0.08);

    // Começa com um peso pequeno da cor mais escura.
    var lab = linear_to_oklab(palette_linear(0.04)) * 0.25;
    var total = 0.25;

    // Cinco bolhas.
    for (var i = 0u; i < 5u; i += 1u) {
        let fi = f32(i);

        // Cada bolha orbita o centro. 1.256637 rad = 72°, então as cinco
        // ficam distribuídas ao redor do círculo. `phase` dá uma volta
        // completa por loop.
        let angle = phase + fi * 1.256637 + seed;

        // O raio respira com uma harmônica inteira da fase (loop fechado).
        let radius = 0.5 + 0.22 * sin(phase * (1.0 + fi) + fi * 2.3 + seed);

        // Centro da bolha. A elipse (x*0.95, y*0.5) acompanha a tela 16:10.
        let center = vec2<f32>(
            cos(angle) * radius * 0.95,
            sin(angle) * radius * 0.5 + (fi - 2.0) * 0.07,
        );

        // Peso gaussiano: perto do centro vale 1, longe cai para 0.
        let d = length(q - center);
        let weight = exp(-d * d * (1.3 + 0.25 * fi));

        // Cada bolha usa uma cor diferente da paleta. O seno dá uma leve
        // variação de cor ao longo do tempo.
        let color = palette_linear(0.28 + 0.15 * fi + 0.05 * sin(phase + fi));
        lab += linear_to_oklab(color) * weight;
        total += weight;
    }

    // Um brilho suave no centro evita o "buraco" escuro no meio.
    let center_glow = exp(-dot(q, q) * 1.5);
    lab += linear_to_oklab(palette_linear(0.62)) * center_glow * 0.35;
    total += center_glow * 0.35;

    // Divide pelo peso total: média ponderada, ou seja,
    //   (w0*cor0 + w1*cor1 + ... + wN*corN) / (w0 + w1 + ... + wN).
    // É por isso que `total` acumula os mesmos pesos que `lab`.
    let col = oklab_to_linear(lab / total);
    return col * (0.9 + 0.1 * sin(phase)) * intensity;
}

// ----------------------------------------------------------------------------
// ONDAS — camadas de dunas coloridas
// ----------------------------------------------------------------------------
fn waves_col(
    p: vec2<f32>,
    phase: f32,
    seed: f32,
    scale: f32,
    intensity: f32,
) -> vec3<f32> {
    let q = p * vec2<f32>(1.0, 1.05) * scale;

    // Céu/fundo: degradê vertical escuro.
    var col = mix(
        palette_linear(0.04),
        palette_linear(0.22),
        smoothstep(-0.8, 0.9, q.y),
    ) * 0.6;

    // Quatro camadas, de baixo para cima.
    for (var i = 0u; i < 4u; i += 1u) {
        let fi = f32(i);

        // Altura base da camada: -0.55, -0.25, 0.05, 0.35.
        let base_y = -0.55 + 0.30 * fi;

        // A crista é uma soma de duas ondas + ruído. As fases usam
        // múltiplos inteiros (1+fi), então o loop fecha.
        let wave = 0.12 * sin(q.x * (0.8 + 0.12 * fi) + phase * (1.0 + fi) + seed + fi * 1.6)
            + 0.06 * sin(q.x * (1.9 + 0.2 * fi) - phase * (1.0 + fi) + fi * 2.3)
            + (fbm(vec2<f32>(q.x * 1.1 + seed, fi * 3.1 + sin(phase))) - 0.5) * 0.10;

        // Distância vertical até a crista.
        let d = q.y - (base_y + wave);

        // `fill` vale 1 abaixo da crista e 0 acima. O 1.0 - smoothstep
        // inverte a transição e mantém as bordas do smoothstep em ordem
        // crescente (a especificação do WGSL exige e0 < e1).
        let fill = 1.0 - smoothstep(-0.06, 0.06, d);

        // Brilho fino na crista (gaussiana estreita).
        let crest = exp(-abs(d) * 16.0);

        let color = palette_linear(0.32 + 0.14 * fi);
        col = mix(col, color, fill * 0.38);
        col += palette_linear(0.82 + 0.04 * fi) * crest * 0.24 * intensity;
    }

    // Brilho atmosférico perto do centro.
    let glow = exp(-abs(q.y + 0.1) * 2.0) * 0.08;
    col += palette_linear(0.9) * glow * intensity;
    return col;
}

// ----------------------------------------------------------------------------
// NÉVOA — nuvem de cor difusa
// ----------------------------------------------------------------------------
fn haze_col(
    p: vec2<f32>,
    phase: f32,
    seed: f32,
    scale: f32,
    intensity: f32,
) -> vec3<f32> {
    let q = p * vec2<f32>(1.0, 1.1) * scale;

    // Dois deslocamentos periódicos para o ruído parecer que está vivo.
    let drift = vec2<f32>(sin(phase) * 0.18, cos(phase) * 0.12);

    // Duas escalas de fbm5: uma grande (nuvem) e uma menor (detalhe).
    let n1 = fbm5(q * 0.8 + vec2<f32>(seed, seed * 0.7) + drift);
    let n2 = fbm5(q * 1.35 - vec2<f32>(seed * 0.4, seed) + vec2<f32>(cos(phase), sin(phase)) * 0.14);

    // Combina os dois ruídos e normaliza para 0..1 com smoothstep.
    let t = smoothstep(0.08, 0.92, n1 * 0.72 + n2 * 0.28);

    // Gradiente escuro -> médio. Depois adiciona a cor clara nas partes
    // onde o ruído fino é alto.
    var col = mix(palette_linear(0.05), palette_linear(0.55), t);
    col = mix(col, palette_linear(0.9), smoothstep(0.6, 1.0, n2) * 0.30);

    // Escurece a base para dar profundidade.
    col *= 0.70 + 0.30 * smoothstep(-1.0, 1.0, q.y);

    // Faixa de brilho ondulada, bem sutil.
    let glow = exp(-abs(q.y - 0.2 - 0.08 * sin(q.x * 1.2 + phase)) * 3.5);
    col += palette_linear(0.95) * glow * 0.06 * intensity;
    return col;
}

// ----------------------------------------------------------------------------
// PRISMA — faixas diagonais suaves
// ----------------------------------------------------------------------------
fn prism_col(
    p: vec2<f32>,
    phase: f32,
    seed: f32,
    scale: f32,
    intensity: f32,
) -> vec3<f32> {
    let q = p * vec2<f32>(1.0, 1.1) * scale;

    // Direção diagonal (0.55 rad ~ 31.5°). `dot(q, direction)` projeta o
    // pixel nesse eixo; o resultado é uma coordenada 1D ao longo da diagonal.
    let angle = 0.55;
    let direction = vec2<f32>(cos(angle), sin(angle));
    let coord = dot(q, direction) + seed * 0.05;

    // Duas ondas senoidais nessa coordenada 1D. Frequências e fases
    // diferentes criam bandas de espessuras variadas. A fase usa
    // multiplicadores inteiros para o loop fechar.
    let wave1 = 0.5 + 0.5 * sin(coord * 1.5 + phase);
    let wave2 = 0.5 + 0.5 * sin(coord * 2.7 - phase * 2.0 + 1.3);
    let t = smoothstep(0.08, 0.92, wave1);

    // Gradiente escuro -> médio e um toque de cor clara na segunda onda.
    var col = mix(palette_linear(0.06), palette_linear(0.62), t);
    col = mix(col, palette_linear(0.9), smoothstep(0.55, 1.0, wave2) * 0.35);

    // Brilho nas cristas: pow(..., 14) deixa só os picos bem finos.
    let crest = pow(max(0.0, sin(coord * 1.5 + phase)), 14.0);
    col += palette_linear(0.98) * crest * 0.10 * intensity;
    col *= 0.85 + 0.15 * smoothstep(-1.0, 1.0, q.y);
    return col;
}

// ----------------------------------------------------------------------------
// SELEÇÃO DO ESTILO
// ----------------------------------------------------------------------------
// Recebe o índice do estilo e devolve a cor daquele estilo. Fica separado
// para o modo barras poder chamar estilos diferentes por barra.
fn style_color(
    style: u32,
    p: vec2<f32>,
    phase: f32,
    seed: f32,
    scale: f32,
    intensity: f32,
) -> vec3<f32> {
    if (style == 0u) {
        return horizon_col(p, phase, seed, scale, intensity);
    } else if (style == 1u) {
        return aurora_col(p, phase, seed, scale, intensity);
    } else if (style == 2u) {
        return mesh_col(p, phase, seed, scale, intensity);
    } else if (style == 3u) {
        return waves_col(p, phase, seed, scale, intensity);
    } else if (style == 4u) {
        return haze_col(p, phase, seed, scale, intensity);
    }
    return prism_col(p, phase, seed, scale, intensity);
}

// ----------------------------------------------------------------------------
// PONTO DE ENTRADA
// ----------------------------------------------------------------------------
@fragment
fn fs_main(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    // Coordenadas de tela e tempo do loop.
    let p = screen_p(frag.xy);
    let phase = TAU * loop_time(u.time, u.params.x);

    // O estilo vem como float no uniforme; +0.5 e cast para u32 arredondam.
    let style = u32(u.params2.y + 0.5);

    var col: vec3<f32>;
    if (u.bars.x > 0.5) {
        // MODO BARRAS
        // u01 = posição horizontal normalizada, 0..1.
        let aspect = u.resolution.x / u.resolution.y;
        let u01 = clamp((p.x / aspect + 1.0) * 0.5, 0.0, 0.99999);

        // Divide a tela em 4 células e descobre qual barra e onde dentro dela.
        let cell = u01 * 4.0;
        let idx = u32(floor(cell));
        let local = fract(cell);

        // Máscara de borda: abre um gap no começo/fim de cada barra. A
        // suavidade controla o tamanho da transição.
        let gap = u.bars.y;
        let soft = max(u.bars.z, 0.001);
        let edge = smoothstep(gap * 0.5, gap * 0.5 + soft, local)
            * (1.0 - smoothstep(1.0 - gap * 0.5 - soft, 1.0 - gap * 0.5, local));

        // Cada barra tem seu próprio estilo, paleta, defasagem e espelho.
        let bar_style = u32(u.bar_params[idx].x + 0.5);
        let bar_enabled = u.bar_params[idx].y;
        let bar_phase = phase + u.bar_params[idx].z * TAU;

        // Comprime a cena inteira para dentro da largura da barra. O x vai
        // de -aspect/4 a +aspect/4 em vez de -aspect a +aspect.
        var p_bar = vec2<f32>((local * 2.0 - 1.0) * (aspect / 4.0), p.y);
        if (u.bar_params[idx].w > 0.5) {
            p_bar.x = -p_bar.x;
        }

        // A paleta usada por palette_linear vem de g_bar.
        g_bar = idx;
        let bar_col = style_color(bar_style, p_bar, bar_phase, u.seed, u.params.y, u.params.z);

        // Gap escuro para separar visualmente as barras.
        let gap_col = palette_linear(0.02) * 0.2;
        col = mix(gap_col, bar_col, edge * bar_enabled);
    } else {
        // MODO NORMAL: um estilo na tela toda, paleta 0 (global).
        g_bar = 0u;
        col = style_color(style, p, phase, u.seed, u.params.y, u.params.z);
    }

    // Máscaras comuns + tone mapping + sRGB.
    col = apply_masks(col, p);
    return finish(col);
}
