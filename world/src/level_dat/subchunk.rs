pub struct SubChunk {

}

impl SubChunk {
    pub fn load(mut bytes: Vec<u8>) -> Option<SubChunk> {
        let ver = bytes.pop().expect("Missing subchunk version");
        match ver {
            8 | 9 => {
                let storage_layers = bytes.pop().expect("Missing storage layers");
                if ver == 9 {
                    let y_index = bytes.pop().expect("Missing Y index");
                }
                let palette_type = bytes.pop().expect("Missing palette type");
                let bits_per_block = palette_type >> 1;

                println!("{}", bits_per_block);
                todo!();
            },

            // 1 => {
            //     todo!("Subchunk V1");
            // }

            a => {println!("Unsupported subchunk version {}", a); return None;}
        }
    }
}