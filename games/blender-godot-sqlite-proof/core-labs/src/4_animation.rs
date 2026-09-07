#[cfg(test)]
mod tests {
    use glam_ozz::Mat4;
    use ozz_animation_rs::{Animation, LocalToModelJob, SamplingContext, SamplingJob, Skeleton, SoaTransform};
    use std::{cell::RefCell, rc::Rc};

    fn assets() -> (Rc<Skeleton>, Rc<Animation>) {
        let skeleton = Rc::new(Skeleton::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/ozz/0_skeleton.ozz")).unwrap());
        let animation = Rc::new(Animation::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/ozz/1_animation.ozz")).unwrap());
        assert_eq!(skeleton.num_joints(), animation.num_tracks());
        (skeleton, animation)
    }

    fn run_jobs(sampling: &mut SamplingJob, skeleton: Rc<Skeleton>, locals: Rc<RefCell<Vec<SoaTransform>>>, models: Rc<RefCell<Vec<Mat4>>>, ratio: f32) -> Vec<[f32; 16]> {
        sampling.set_ratio(ratio);
        sampling.run().unwrap();
        let mut model_job = LocalToModelJob::default();
        model_job.set_skeleton(skeleton);
        model_job.set_input(locals);
        model_job.set_output(models.clone());
        model_job.run().unwrap();
        models.borrow().iter().map(Mat4::to_cols_array).collect()
    }

    fn sample_reused(ratios: &[f32]) -> Vec<Vec<[f32; 16]>> {
        let (skeleton, animation) = assets();
        let locals = Rc::new(RefCell::new(vec![SoaTransform::default(); skeleton.num_soa_joints()]));
        let models = Rc::new(RefCell::new(vec![Mat4::IDENTITY; skeleton.num_joints()]));
        let mut sampling = SamplingJob::default();
        sampling.set_animation(animation.clone());
        sampling.set_context(SamplingContext::new(animation.num_tracks()));
        sampling.set_output(locals.clone());
        ratios.iter().map(|&ratio| run_jobs(&mut sampling, skeleton.clone(), locals.clone(), models.clone(), ratio)).collect()
    }

    fn sample_fresh(ratios: &[f32]) -> Vec<Vec<[f32; 16]>> {
        let (skeleton, animation) = assets();
        let locals = Rc::new(RefCell::new(vec![SoaTransform::default(); skeleton.num_soa_joints()]));
        let models = Rc::new(RefCell::new(vec![Mat4::IDENTITY; skeleton.num_joints()]));
        ratios.iter().map(|&ratio| {
            let mut sampling = SamplingJob::default();
            sampling.set_animation(animation.clone());
            sampling.set_context(SamplingContext::new(animation.num_tracks()));
            sampling.set_output(locals.clone());
            run_jobs(&mut sampling, skeleton.clone(), locals.clone(), models.clone(), ratio)
        }).collect()
    }

    #[test]
    fn real_ozz_fixture_backward_resample_matches_fresh_context_model_matrices() {
        let ratios = [0.0, 0.2, 0.7, 0.95, 0.35, 0.1, 0.8];
        let reused = sample_reused(&ratios);
        let fresh = sample_fresh(&ratios);
        assert_eq!(reused, fresh);
        assert!(reused.iter().all(|pose| pose.len() > 1));
        assert_ne!(reused[1], reused[2]);
    }
}
