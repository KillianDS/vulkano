// Copyright (c) 2017 The vulkano developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use descriptor::pipeline_layout::PipelineLayoutAbstract;
use descriptor::pipeline_layout::PipelineLayoutPushConstantsCompatible;

/// Checks whether push constants are compatible with the pipeline.
pub fn check_push_constants_validity<Pl, Pc>(pipeline: &Pl, push_constants: &Pc)
                                             -> Result<(), CheckPushConstantsValidityError>
    where Pl: ?Sized + PipelineLayoutAbstract + PipelineLayoutPushConstantsCompatible<Pc>,
          Pc: ?Sized
{
    if !pipeline.is_compatible(push_constants) {
        return Err(CheckPushConstantsValidityError::IncompatiblePushConstants);
    }

    Ok(())
}

simple_error!(CheckPushConstantsValidityError {
    IncompatiblePushConstants: "the push constants are incompatible with the pipeline layout"
});
