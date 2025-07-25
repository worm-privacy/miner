pub fn check_required_files(params_dir: &std::path::Path) -> Result<(), anyhow::Error> {
    let required_files = [
        "proof_of_burn.dat",
        "proof_of_burn.zkey",
        "spend.dat",
        "spend.zkey",
    ];

    for req_file in required_files {
        let full_path = params_dir.join(req_file);
        if !std::fs::exists(&full_path)? {
            panic!(
                "File {} does not exist! Make sure you have downloaded all required files through `make download_params`!",
                full_path.display()
            );
        }
    }
    Ok(())
}
