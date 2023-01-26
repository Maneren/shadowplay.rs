#[allow(dead_code)]
pub fn argb_to_yuv420(width: usize, height: usize, src: &[u8]) -> Vec<u8> {
  let frame_size = width * height;
  let u_size = frame_size / 4;

  let mut yuv = vec![0; frame_size * 3 / 2];

  let mut u_index = frame_size;
  let mut v_index = u_index + u_size;

  let mut column_index = 0;
  let mut row_index = 0;

  for (y_index, [b, g, r, _]) in src.array_chunks().enumerate() {
    let r = i32::from(*r);
    let g = i32::from(*g);
    let b = i32::from(*b);

    yuv[y_index] = clamp((66 * r + 129 * g + 25 * b + 128) / 256 + 16);

    if column_index % 2 == 0 && row_index % 2 == 0 {
      yuv[u_index] = clamp((-38 * r - 74 * g + 112 * b + 128) / 256 + 128);
      yuv[v_index] = clamp((112 * r - 94 * g - 18 * b + 128) / 256 + 128);

      u_index += 1;
      v_index += 1;
    }

    column_index += 1;

    if column_index == width {
      row_index += 1;
      column_index = 0;
    }
  }

  yuv
}

#[allow(dead_code)]
pub fn argb_to_yuv420_with_subsampling(width: usize, height: usize, src: &[u8]) -> Vec<u8> {
  let frame_size = width * height;
  let u_size = frame_size / 4;

  let mut yuv = vec![0; frame_size * 3 / 2];

  let mut u_index = frame_size;
  let mut v_index = u_index + u_size;

  let mut column_index = 0;
  let mut row_index = 0;

  let get_pixel = |idx| {
    let idx = idx * 4;

    let r = i32::from(src[idx + 2]);
    let g = i32::from(src[idx + 1]);
    let b = i32::from(src[idx]);

    [r, g, b]
  };

  let calc_y = |[r, g, b]: [i32; 3]| clamp((66 * r + 129 * g + 25 * b + 128) / 256 + 16);
  let calc_u = |[r, g, b]: [i32; 3]| (-38 * r - 74 * g + 112 * b + 128) / 256 + 128;
  let calc_v = |[r, g, b]: [i32; 3]| (112 * r - 94 * g - 18 * b + 128) / 256 + 128;

  for i in 0..frame_size {
    let pixel = get_pixel(i);

    yuv[i] = calc_y(pixel);

    if column_index % 2 == 0 && row_index % 2 == 0 {
      // use average subsampling
      let sample = [
        pixel,
        get_pixel(i + 1),
        get_pixel(i + width),
        get_pixel(i + width + 1),
      ];

      let u = sample.into_iter().map(calc_u).sum::<i32>() / 4;
      let v = sample.into_iter().map(calc_v).sum::<i32>() / 4;

      yuv[u_index] = clamp(u);
      yuv[v_index] = clamp(v);

      u_index += 1;
      v_index += 1;
    }

    column_index += 1;

    if column_index == width {
      row_index += 1;
      column_index = 0;
    }
  }

  yuv
}

#[allow(dead_code)]
pub fn argb_to_yuv444(width: usize, height: usize, src: &[u8]) -> Vec<u8> {
  let frame_size = width * height;

  let mut yuv = vec![0; frame_size * 3];

  let u_offset = frame_size;
  let v_offset = u_offset + frame_size;

  for (y_index, [b, g, r, _]) in src.array_chunks().enumerate() {
    let r = i32::from(*r);
    let g = i32::from(*g);
    let b = i32::from(*b);

    yuv[y_index] = clamp((66 * r + 129 * g + 25 * b + 128) / 256 + 16);
    yuv[y_index + u_offset] = clamp((-38 * r - 74 * g + 112 * b + 128) / 256 + 128);
    yuv[y_index + v_offset] = clamp((112 * r - 94 * g - 18 * b + 128) / 256 + 128);
  }

  yuv
}

#[allow(dead_code)]
fn clamp(x: i32) -> u8 {
  x.min(255).max(0) as u8
}
