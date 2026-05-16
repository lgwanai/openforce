use sqlx::PgPool;
use tonic::{Request, Response, Status};
use uuid::Uuid;
use sha2::Sha256; use digest::Digest;

use openforce_proto::swarmos::v1::{
    project_tool_service_server::ProjectToolService,
    approval_service_server::ApprovalService,
    ReadProjectFileRequest, ReadProjectFileResponse,
    ReadProjectTreeRequest, ReadProjectTreeResponse,
    ReadProjectTreeSuccess,
    SubmitProjectPatchRequest, SubmitProjectPatchResponse,
    SubmitProjectPatchSuccess,
    DeleteProjectFileRequest, DeleteProjectFileResponse,
    DeleteProjectFileSuccess,
    CreateApprovalRequestRequest, CreateApprovalRequestResponse,
    GetApprovalRequestRequest, GetApprovalRequestResponse,
    ApproveApprovalRequestRequest, ApproveApprovalRequestResponse,
    RejectApprovalRequestRequest, RejectApprovalRequestResponse,
    ConsumeApprovalTokenRequest, ConsumeApprovalTokenResponse,
    ProjectFile, PatchClassification as ProtoClassification,
    ToolError, ToolErrorCode, PatchRiskLevel,
};

use openforce_path_acl::PathAcl;
use openforce_patch_classifier::PatchClassifier;

use crate::approval_store::ApprovalStore;

#[derive(Clone)]
pub struct ProjectToolServiceImpl {
    pub pool: PgPool,
}

fn parse_uuid(s: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(s).map_err(|_| Status::invalid_argument("invalid UUID"))
}

fn tool_err(code: ToolErrorCode, msg: &str) -> ToolError {
    ToolError { code: code as i32, message: msg.into(), details: vec![] }
}

#[tonic::async_trait]
impl ProjectToolService for ProjectToolServiceImpl {
    async fn read_project_file(
        &self, r: Request<ReadProjectFileRequest>,
    ) -> Result<Response<ReadProjectFileResponse>, Status> {
        let req = r.into_inner();
        let worker = req.worker.ok_or(Status::invalid_argument("worker required"))?;
        let caps = req.capabilities.ok_or(Status::invalid_argument("capabilities required"))?;

        let acl = PathAcl::new(&caps.allowed_read_paths, &caps.allowed_write_paths, &caps.forbidden_paths);
        if !acl.can_read(&req.path) {
            return Ok(Response::new(ReadProjectFileResponse {
                result: Some(openforce_proto::swarmos::v1::read_project_file_response::Result::Error(
                    tool_err(ToolErrorCode::PathNotAllowed, "read path not allowed"),
                )),
            }));
        }

        // In production, fetch from VFS/storage via snapshot_id
        let content = format!("placeholder content for {}", req.path).into_bytes();
        let sha = hex::encode(Sha256::digest(&content));

        Ok(Response::new(ReadProjectFileResponse {
            result: Some(openforce_proto::swarmos::v1::read_project_file_response::Result::File(
                ProjectFile {
                    path: req.path,
                    content,
                    sha256: sha,
                    snapshot_id: worker.command_id,
                },
            )),
        }))
    }

    async fn read_project_tree(
        &self, r: Request<ReadProjectTreeRequest>,
    ) -> Result<Response<ReadProjectTreeResponse>, Status> {
        let req = r.into_inner();
        let caps = req.capabilities.ok_or(Status::invalid_argument("capabilities required"))?;
        let acl = PathAcl::new(&caps.allowed_read_paths, &caps.allowed_write_paths, &caps.forbidden_paths);
        if !acl.can_read(&req.root_path) {
            return Ok(Response::new(ReadProjectTreeResponse {
                result: Some(openforce_proto::swarmos::v1::read_project_tree_response::Result::Error(
                    tool_err(ToolErrorCode::PathNotAllowed, "read path not allowed"),
                )),
            }));
        }

        // In production, fetch from VFS
        Ok(Response::new(ReadProjectTreeResponse {
            result: Some(openforce_proto::swarmos::v1::read_project_tree_response::Result::Success(
                ReadProjectTreeSuccess { nodes: vec![], snapshot_id: "current".into() },
            )),
        }))
    }

    async fn submit_project_patch(
        &self, r: Request<SubmitProjectPatchRequest>,
    ) -> Result<Response<SubmitProjectPatchResponse>, Status> {
        let req = r.into_inner();
        let worker = req.worker.ok_or(Status::invalid_argument("worker required"))?;
        let caps = req.capabilities.ok_or(Status::invalid_argument("capabilities required"))?;
        let patch = req.patch.ok_or(Status::invalid_argument("patch required"))?;

        let acl = PathAcl::new(&caps.allowed_read_paths, &caps.allowed_write_paths, &caps.forbidden_paths);

        // Check write permissions for each target path
        for path in &patch.target_paths {
            if !acl.can_write(path) {
                return Ok(Response::new(SubmitProjectPatchResponse {
                    result: Some(openforce_proto::swarmos::v1::submit_project_patch_response::Result::Error(
                        tool_err(ToolErrorCode::PathNotAllowed, &format!("write path not allowed: {path}")),
                    )),
                }));
            }
        }

        // Classify the patch
        let classifier = PatchClassifier::new();
        let classification = classifier.classify(
            &patch.target_paths,
            &caps.allowed_read_paths,
            &caps.allowed_write_paths,
            &caps.forbidden_paths,
            0, 0, 0, // lines removed/added/files deleted
        );

        // Reject patches that cannot proceed (forbidden paths, etc.)
        if !classification.can_proceed() {
            return Ok(Response::new(SubmitProjectPatchResponse {
                result: Some(openforce_proto::swarmos::v1::submit_project_patch_response::Result::Error(
                    tool_err(ToolErrorCode::PatchRejected, "patch classified as Reject"),
                )),
            }));
        }

        // If needs approval and no token provided
        if classification.requires_approval && req.approval_token.is_empty() {
            return Ok(Response::new(SubmitProjectPatchResponse {
                result: Some(openforce_proto::swarmos::v1::submit_project_patch_response::Result::PendingHumanApproval(
                    openforce_proto::swarmos::v1::PendingHumanApproval {
                        approval_request: Some(openforce_proto::swarmos::v1::ApprovalRequestView {
                            approval_request_id: Uuid::now_v7().to_string(),
                            tool_name: 3, // write_project_patch
                            patch_risk_level: classification.risk_level as i32,
                            reason_codes: classification.reason_codes.iter().map(|r| *r as i32).collect(),
                            target_paths: patch.target_paths.clone(),
                            base_snapshot_id: patch.base_snapshot_id.clone(),
                            payload_sha256: patch.patch_sha256.clone(),
                            status: 1, // pending_human
                            ..Default::default()
                        }),
                    },
                )),
            }));
        }

        Ok(Response::new(SubmitProjectPatchResponse {
            result: Some(openforce_proto::swarmos::v1::submit_project_patch_response::Result::Success(
                SubmitProjectPatchSuccess {
                    merge_commit_id: Uuid::now_v7().to_string(),
                    new_snapshot_id: Uuid::now_v7().to_string(),
                    classification: Some(ProtoClassification {
                        risk_level: classification.risk_level as i32,
                        reason_codes: classification.reason_codes.iter().map(|r| *r as i32).collect(),
                        requires_approval: classification.requires_approval,
                    }),
                },
            )),
        }))
    }

    async fn delete_project_file(
        &self, r: Request<DeleteProjectFileRequest>,
    ) -> Result<Response<DeleteProjectFileResponse>, Status> {
        let req = r.into_inner();
        let caps = req.capabilities.ok_or(Status::invalid_argument("capabilities required"))?;
        let acl = PathAcl::new(&caps.allowed_read_paths, &caps.allowed_write_paths, &caps.forbidden_paths);

        if !acl.can_delete(&req.target_path) {
            return Ok(Response::new(DeleteProjectFileResponse {
                result: Some(openforce_proto::swarmos::v1::delete_project_file_response::Result::Error(
                    tool_err(ToolErrorCode::PathNotAllowed, "delete path not allowed"),
                )),
            }));
        }

        // Always require approval for deletes
        if req.approval_token.is_empty() {
            return Ok(Response::new(DeleteProjectFileResponse {
                result: Some(openforce_proto::swarmos::v1::delete_project_file_response::Result::PendingHumanApproval(
                    openforce_proto::swarmos::v1::PendingHumanApproval {
                        approval_request: Some(openforce_proto::swarmos::v1::ApprovalRequestView {
                            approval_request_id: Uuid::now_v7().to_string(),
                            tool_name: 4, // delete_project_file
                            patch_risk_level: PatchRiskLevel::Sensitive as i32,
                            target_paths: vec![req.target_path.clone()],
                            base_snapshot_id: req.base_snapshot_id,
                            status: 1, // pending_human
                            ..Default::default()
                        }),
                    },
                )),
            }));
        }

        Ok(Response::new(DeleteProjectFileResponse {
            result: Some(openforce_proto::swarmos::v1::delete_project_file_response::Result::Success(
                DeleteProjectFileSuccess {
                    deleted_path: req.target_path,
                    new_snapshot_id: Uuid::now_v7().to_string(),
                },
            )),
        }))
    }
}

#[derive(Clone)]
pub struct ApprovalServiceImpl {
    pub approval_store: std::sync::Arc<ApprovalStore>,
}

#[tonic::async_trait]
impl ApprovalService for ApprovalServiceImpl {
    async fn create_approval_request(
        &self, r: Request<CreateApprovalRequestRequest>,
    ) -> Result<Response<CreateApprovalRequestResponse>, Status> {
        let req = r.into_inner();
        let worker = req.worker.ok_or(Status::invalid_argument("worker required"))?;
        let class = req.classification.ok_or(Status::invalid_argument("classification required"))?;

        let domain_class = openforce_domain::patch::PatchClassification {
            risk_level: openforce_domain::patch::PatchRiskLevel::Sensitive,
            reason_codes: vec![],
            requires_approval: true,
        };

        let ar = self.approval_store.create_approval_request(
            parse_uuid(&worker.session_id)?, parse_uuid(&worker.task_id)?,
            worker.task_attempt as i32, parse_uuid(&worker.lease_id)?,
            worker.fencing_token, parse_uuid(&worker.worker_spec_id)?,
            "write_project_patch", &req.target_paths,
            &req.base_snapshot_id, &req.payload_sha256,
            &domain_class, 30,
        ).await.map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(CreateApprovalRequestResponse {
            result: Some(openforce_proto::swarmos::v1::create_approval_request_response::Result::ApprovalRequest(
                openforce_proto::swarmos::v1::ApprovalRequestView {
                    approval_request_id: ar.approval_request_id.to_string(),
                    status: 1,
                    ..Default::default()
                },
            )),
        }))
    }

    async fn get_approval_request(
        &self, _r: Request<GetApprovalRequestRequest>,
    ) -> Result<Response<GetApprovalRequestResponse>, Status> {
        Err(Status::unimplemented("get_approval_request"))
    }

    async fn approve_approval_request(
        &self, r: Request<ApproveApprovalRequestRequest>,
    ) -> Result<Response<ApproveApprovalRequestResponse>, Status> {
        let req = r.into_inner();
        let token = self.approval_store.approve_request(
            parse_uuid(&req.approval_request_id)?, &req.approver_id, req.usage_limit,
        ).await.map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(ApproveApprovalRequestResponse {
            result: Some(openforce_proto::swarmos::v1::approve_approval_request_response::Result::ApprovalToken(
                openforce_proto::swarmos::v1::ApprovalBinding {
                    approval_token_id: token.approval_token_id.to_string(),
                    approval_request_id: token.approval_request_id.to_string(),
                    session_id: token.session_id.to_string(),
                    task_id: token.task_id.to_string(),
                    task_attempt: token.task_attempt as u32,
                    lease_id: token.lease_id.to_string(),
                    fencing_token: token.fencing_token,
                    worker_spec_id: token.worker_spec_id.to_string(),
                    usage_limit: token.usage_limit,
                    usage_count: token.usage_count,
                    status: 2, // approved
                    issued_at: None,
                    expires_at: None,
                    ..Default::default()
                },
            )),
        }))
    }

    async fn reject_approval_request(
        &self, _r: Request<RejectApprovalRequestRequest>,
    ) -> Result<Response<RejectApprovalRequestResponse>, Status> {
        Err(Status::unimplemented("reject_approval_request"))
    }

    async fn consume_approval_token(
        &self, r: Request<ConsumeApprovalTokenRequest>,
    ) -> Result<Response<ConsumeApprovalTokenResponse>, Status> {
        let req = r.into_inner();
        let worker = req.worker.ok_or(Status::invalid_argument("worker required"))?;

        let token = self.approval_store.consume_token(
            parse_uuid(&req.approval_token)?,
            parse_uuid(&worker.session_id)?, parse_uuid(&worker.task_id)?,
            worker.task_attempt as i32, parse_uuid(&worker.lease_id)?,
            worker.fencing_token,
            &req.base_snapshot_id, &req.payload_sha256,
        ).await.map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(ConsumeApprovalTokenResponse {
            result: Some(openforce_proto::swarmos::v1::consume_approval_token_response::Result::ApprovalBinding(
                openforce_proto::swarmos::v1::ApprovalBinding {
                    approval_token_id: token.approval_token_id.to_string(),
                    status: 6, // consumed
                    ..Default::default()
                },
            )),
        }))
    }
}
