// ============================================================================
// POST-PROCESSAMENTO — correção de cor global
// ============================================================================
// A cena já foi desenhada numa textura (scene_texture). Este shader roda
// depois, uma vez por pixel, lê a cor daquele pixel e dos vizinhos e aplica
// a correção de cor. É o mesmo caminho para preview, PNG e MP4.
//
// Por que não fazer isso direto na cena? Porque nitidez precisa comparar um
// pixel com os vizinhos, e a cena não tem acesso à imagem já pronta. Um passe
// separado também deixa a correção igual para todas as cenas.
//
// O grupo 0, binding 0 (Uniforms) vem do common.wgsl. Aqui adicionamos:
//   binding 1 = a textura da cena
//   binding 2 = o sampler (como ler a textura)
// ============================================================================

@group(0) @binding(1) var scene_texture: texture_2d<f32>;
@group(0) @binding(2) var scene_sampler: sampler;
@group(0) @binding(3) var text_texture: texture_2d_array<f32>;
@group(0) @binding(4) var text_sampler: sampler;

// ----------------------------------------------------------------------------
// MAPAS DE RUÍDO DO WARP
// ----------------------------------------------------------------------------
// Todos devolvem um deslocamento 2D aproximadamente em -1..1. O post divide
// esse valor por uma escala e multiplica pela força escolhida.
fn voronoi(p: vec2<f32>) -> vec2<f32> {
    let cell = floor(p);
    let local = fract(p);
    var best = vec2<f32>(0.0);
    var best_dist = 8.0;
    // Olha as 9 células em volta para achar o ponto mais próximo.
    for (var y = -1; y <= 1; y += 1) {
        for (var x = -1; x <= 1; x += 1) {
            let offset = vec2<f32>(f32(x), f32(y));
            let random = vec2<f32>(
                hash21(cell + offset),
                hash21(cell + offset + vec2<f32>(17.0, 9.0)),
            );
            let delta = offset + random - local;
            let dist = dot(delta, delta);
            if (dist < best_dist) {
                best_dist = dist;
                best = delta;
            }
        }
    }
    return best;
}

fn warp_offset(uv: vec2<f32>, phase: f32, seed: f32, scale: f32, map: u32) -> vec2<f32> {
    let aspect = u.resolution.x / u.resolution.y;
    let centered = vec2<f32>((uv.x * 2.0 - 1.0) * aspect, uv.y * 2.0 - 1.0);
    let drift = vec2<f32>(sin(phase) * 0.10, cos(phase) * 0.10);

    if (map == 0u) {
        // FBM: duas nuvens de ruído, uma para x e outra para y.
        let q = uv * scale * 3.0 + vec2<f32>(seed, seed * 0.7) + drift;
        let dx = fbm5(q) - 0.5;
        let dy = fbm5(q + vec2<f32>(5.2, 1.3)) - 0.5;
        return vec2<f32>(dx, dy) * 2.0;
    } else if (map == 1u) {
        // Turbulência: dobra o ruído com abs(), criando cristas.
        let q = uv * scale * 3.0 + vec2<f32>(seed, seed * 0.7) + drift;
        let dx = abs(fbm5(q) * 2.0 - 1.0) - 0.5;
        let dy = abs(fbm5(q + vec2<f32>(3.7, 9.1)) * 2.0 - 1.0) - 0.5;
        return vec2<f32>(dx, dy) * 2.0;
    } else if (map == 2u) {
        // Voronoi: desloca em direção ao ponto de célula mais próximo.
        let q = uv * scale * 5.0 + vec2<f32>(seed) + drift * 0.3;
        return voronoi(q) * 0.5;
    } else if (map == 3u) {
        // Ondas: senos em x e y, defasados.
        return vec2<f32>(
            sin(centered.y * scale * 6.0 + phase),
            cos(centered.x * scale * 6.0 - phase),
        );
    } else if (map == 4u) {
        // Swirl: gira em volta do centro, com força que cresce com o raio.
        let radius = length(centered) + 0.001;
        let direction = centered / radius;
        let amount = sin(radius * scale * 5.0 - phase) * smoothstep(0.0, 0.5, radius);
        return vec2<f32>(-direction.y, direction.x) * amount;
    }

    // Ripple: empurra para fora/para dentro em anéis concêntricos.
    let radius = length(centered) + 0.001;
    let direction = centered / radius;
    return direction * sin(radius * scale * 12.0 - phase);
}

@fragment
fn fs_post(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    // uv vai de 0..1. É o sistema usado para amostrar texturas.
    let uv = frag.xy / u.resolution;

    // Distância de um pixel em UV. Usamos a referência 1920x1200 de propósito:
    // assim a nitidez parece igual no preview (960x600) e no export.
    let texel = vec2<f32>(1.0 / 1920.0, 1.0 / 1200.0);

    // Warp: desloca o UV antes de amostrar a cena. Se estiver desligado,
    // o deslocamento é zero e tudo segue igual.
    let warp_phase = TAU * loop_time(u.time, u.warp2.x);
    let warp = warp_offset(uv, warp_phase, u.warp2.y, u.warp.w, u32(u.warp.y + 0.5))
        * u.warp.z
        * u.warp.x;
    let warped_uv = clamp(uv + warp, vec2<f32>(0.001), vec2<f32>(0.999));

    // Lê o pixel central e os quatro vizinhos. textureSample interpola
    // usando o sampler (aqui, filtro linear).
    let center = textureSample(scene_texture, scene_sampler, warped_uv).rgb;
    let north = textureSample(scene_texture, scene_sampler, warped_uv + vec2<f32>(0.0, -texel.y)).rgb;
    let south = textureSample(scene_texture, scene_sampler, warped_uv + vec2<f32>(0.0, texel.y)).rgb;
    let east = textureSample(scene_texture, scene_sampler, warped_uv + vec2<f32>(texel.x, 0.0)).rgb;
    let west = textureSample(scene_texture, scene_sampler, warped_uv + vec2<f32>(-texel.x, 0.0)).rgb;

    // Média dos vizinhos = versão borrada. É a base do "unsharp mask":
    // center - blur realça as diferenças, ou seja, as bordas.
    let blur = (north + south + east + west) * 0.25;

    // Nitidez: 0 devolve o center; 1 realça bastante. A conta é
    //   center + (center - blur) * força.
    // O termo (center - blur) é a "alta frequência" (detalhe/edge). Somar
    // isso de volta realça a borda. O clamp evita valores fora de 0..1.
    var graded = center + (center - blur) * u.post2.w * 2.0;
    graded = clamp(graded, vec3<f32>(0.0), vec3<f32>(1.0));

    // A textura guarda sRGB (pronto para tela). Para fazer contas de luz,
    // decodificamos para linear.
    var linear = srgb_to_linear(graded);

    // ------------------------------------------------------------------------
    // TEXTO
    // ------------------------------------------------------------------------
    // O texto também é amostrado no UV já distorcido pelo warp, então ele
    // acompanha a distorção da cena. Compomos em linear, antes da correção
    // de cor, para o grade valer para o texto também.
    let aspect = u.resolution.x / u.resolution.y;
    let warped_p = vec2<f32>(
        (warped_uv.x * 2.0 - 1.0) * aspect,
        -(warped_uv.y * 2.0 - 1.0),
    );
    for (var i = 0u; i < 4u; i += 1u) {
        if (u.text_params[i].x < 0.5) {
            continue;
        }
        let line_height = max(u.text_params[i].w, 0.001);
        // A textura tem v=0 no topo e v=1 embaixo. Como warped_p.y é
        // positivo para cima, invertemos o sinal para o texto não sair
        // de cabeça para baixo.
        let v = 0.5 - (warped_p.y - u.text_params[i].z) / line_height;
        if (v < 0.0 || v > 1.0) {
            continue;
        }
        // O atlas tem 2048x256 (proporção 8:1). O texto se repete na
        // horizontal; fract() faz o letreiro voltar ao começo.
        let tile_width = line_height * 8.0;
        let text_u = fract(warped_p.x / tile_width + u.text_params[i].y);
        let coverage = textureSample(
            text_texture,
            text_sampler,
            vec2<f32>(text_u, v),
            i32(i),
        ).a * u.text_colors[i].a;
        let text_rgb = srgb_to_linear(u.text_colors[i].rgb);
        linear = mix(linear, text_rgb, clamp(coverage, 0.0, 1.0));
    }

    // Exposição em EV: +1 dobra a luz, -1 corta pela metade.
    // exp2(x) é 2 elevado a x: exp2(1)=2, exp2(0)=1, exp2(-1)=0.5.
    linear *= exp2(u.post.x);

    // Brilho: soma um pouquinho em linear. O fator 0.2 limita o efeito:
    // com o slider em 0.5, soma 0.1; em -0.5, subtrai 0.1.
    linear += vec3<f32>(u.post.y) * 0.2;

    // Contraste em torno do cinza médio (0.18 em linear).
    // A conta é (cor - 0.18) * (1 + contraste) + 0.18.
    // Com contraste 0, nada muda. Com +1, a distância do cinza dobra.
    // Com -1, tudo colapsa para o próprio cinza.
    linear = (linear - 0.18) * (1.0 + u.post.z) + 0.18;

    // Saturação: luma é o brilho percebido. A conta
    //   mix(cinza, cor, saturação)
    // é cinza*(1-s) + cor*s. s=0 -> preto e branco; s=1 -> normal;
    // s=2 -> cor + (cor - cinza), ou seja, o dobro de distância do cinza.
    let luma = dot(linear, vec3<f32>(0.2126, 0.7152, 0.0722));
    linear = mix(vec3<f32>(luma), linear, u.post.w);

    // Temperatura: mexe no vermelho e no azul (azul <-> laranja).
    // Tint: mexe no verde (verde <-> magenta).
    let temperature = u.post2.x;
    let tint = u.post2.y;
    linear *= vec3<f32>(
        1.0 + temperature * 0.15 - tint * 0.06,
        1.0 + tint * 0.12,
        1.0 - temperature * 0.15 - tint * 0.06,
    );

    // Gamma: a conta é x^(1/gamma).
    //   gamma = 1   -> x^1 = x (neutro)
    //   gamma = 2   -> x^0.5 = sqrt(x): clareia os meios-tons
    //   gamma = 0.5 -> x^2: escurece os meios-tons
    // O max evita divisão por zero se o valor chegar corrompido.
    linear = pow(max(linear, vec3<f32>(0.0)), vec3<f32>(1.0 / max(u.post2.z, 0.01)));
    linear = clamp(linear, vec3<f32>(0.0), vec3<f32>(1.0));

    // Volta para sRGB (o que a tela/PNG/MP4 esperam).
    var srgb = linear_to_srgb(linear);

    // Dither: um ruído minúsculo de ±0.5/255 antes de gravar em 8 bits.
    // Sem isso, gradientes suaves criam faixas visíveis no projetor.
    // O hash depende do pixel e da seed; o mesmo frame sempre dá o mesmo
    // resultado, então o vídeo não cintila.
    srgb += (hash21(frag.xy + u.seed * 17.0) - 0.5) / 255.0;
    return vec4<f32>(clamp(srgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
